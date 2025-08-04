use crate::aarch64::interrupts::IrqMutex;
use crate::{print, println};
use core::fmt::{Debug, Formatter};
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};
use core::ptr::{drop_in_place, slice_from_raw_parts, slice_from_raw_parts_mut, write_bytes};
use core::{fmt, mem};
use elain::Align;
use zerocopy::FromZeros;

pub const PAGE_SIZE: usize = 4096;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct PhyAddr(pub usize);

impl PhyAddr {
    pub fn from_virt<T>(addr: *const T) -> Self {
        Self(addr as usize & 0x0000_00ff_ffff_ffff)
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct PhySlice {
    pub base: PhyAddr,
    pub len: usize,
}

impl Debug for PhyAddr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "0p{:x}", self.0)
    }
}

impl Debug for PhySlice {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "0p{:x}..0p{:x}", self.base.0, self.base.0 + self.len)
    }
}

impl PhyAddr {
    pub unsafe fn virt<T>(&self) -> *const T {
        (self.0 | 0xffff_ff00_0000_0000u64 as usize) as *const T
    }

    pub unsafe fn virt_mut<T>(&self) -> *mut T {
        (self.0 | 0xffff_ff00_0000_0000u64 as usize) as *mut T
    }

    pub unsafe fn virt_dev<T>(&self) -> *const T {
        (self.0 | 0xffff_ff80_0000_0000u64 as usize) as *const T
    }

    pub unsafe fn virt_dev_mut<T>(&self) -> *mut T {
        (self.0 | 0xffff_ff80_0000_0000u64 as usize) as *mut T
    }
}

impl PhySlice {
    pub unsafe fn virt(&self) -> &'static [u8] {
        &*slice_from_raw_parts(self.base.virt(), self.len)
    }

    pub unsafe fn virt_mut(&self) -> &'static mut [u8] {
        &mut *slice_from_raw_parts_mut(self.base.virt_mut(), self.len)
    }

    pub unsafe fn virt_dev(&self) -> &'static [u8] {
        &*slice_from_raw_parts(self.base.virt_dev(), self.len)
    }

    pub unsafe fn virt_dev_mut(&self) -> &'static mut [u8] {
        &mut *slice_from_raw_parts_mut(self.base.virt_dev_mut(), self.len)
    }
}

struct BitmapIterator<'a> {
    bitmap: &'a [u64],
    idx: usize,
}

impl<'a> Iterator for BitmapIterator<'a> {
    type Item = bool;

    fn next(&mut self) -> Option<Self::Item> {
        if self.idx >= self.bitmap.len() * 64 {
            return None;
        }
        let bit = self.bitmap[self.idx / 64] & (1 << (self.idx % 64)) != 0;
        self.idx += 1;
        Some(bit)
    }
}

struct Bitmap<const SIZE: usize> {
    // TODO-32bit: u64 -> usize
    bitmap: [u64; SIZE],
}

impl<const SIZE: usize> Bitmap<SIZE> {
    const fn new() -> Self {
        Self { bitmap: [0; SIZE] }
    }

    // TODO: find_hole_big (>64 pages)
    fn find_hole(&self, page_count: usize) -> Option<usize> {
        let mut page_num = 0;
        let mut hole_count = 0;
        for i in 0..self.bitmap.len() {
            let bitmap = self.bitmap[i];
            for j in 0..64 {
                if bitmap & (1 << j) == 0 {
                    // Empty page
                    // println!("empty page {}", i * 64 + j);
                    if hole_count == 0 {
                        page_num = i * 64 + j;
                    }
                    hole_count += 1;
                    if hole_count == page_count {
                        return Some(page_num);
                    }
                } else {
                    page_num += hole_count;
                    hole_count = 0;
                }
            }
        }
        None
    }

    pub fn alloc(&mut self, page_count: usize) -> Option<usize> {
        if let Some(page_num) = self.find_hole(page_count) {
            self.mark_allocated(page_num, page_count);
            Some(page_num)
        } else {
            None
        }
    }

    pub fn mark_allocated(&mut self, mut page_num: usize, mut page_count: usize) {
        while page_num % 64 != 0 && page_count > 0 {
            debug_assert!(
                self.bitmap[page_num / 64] & (1 << (page_num % 64)) == 0,
                "double alloc"
            );
            self.bitmap[page_num / 64] |= 1 << (page_num % 64);
            page_num += 1;
            page_count -= 1;
        }
        while page_count >= 64 {
            debug_assert!(self.bitmap[page_num / 64] == 0, "double alloc");
            self.bitmap[page_num / 64] = 0xffffffffffffffff;
            page_num += 64;
            page_count -= 64;
        }
        while page_count > 0 {
            debug_assert!(
                self.bitmap[page_num / 64] & (1 << (page_num % 64)) == 0,
                "double alloc"
            );
            self.bitmap[page_num / 64] |= 1 << (page_num % 64);
            page_num += 1;
            page_count -= 1;
        }
    }

    pub fn free(&mut self, mut page_num: usize, mut page_count: usize) {
        while page_num % 64 != 0 && page_count > 0 {
            debug_assert!(
                self.bitmap[page_num / 64] & (1 << (page_num % 64)) != 0,
                "double free"
            );
            self.bitmap[page_num / 64] &= !(1 << (page_num % 64));
            page_num += 1;
            page_count -= 1;
        }
        while page_count >= 64 {
            debug_assert!(
                self.bitmap[page_num / 64] == 0xffffffffffffffff,
                "double free"
            );
            self.bitmap[page_num / 64] = 0;
            page_num += 64;
            page_count -= 64;
        }
        while page_count > 0 {
            debug_assert!(
                self.bitmap[page_num / 64] & (1 << (page_num % 64)) != 0,
                "double free"
            );
            self.bitmap[page_num / 64] &= !(1 << (page_num % 64));
            page_num += 1;
            page_count -= 1;
        }
    }

    pub fn bit_capacity(&self) -> usize {
        self.bitmap.len() * 64
    }

    pub fn iter(&self) -> BitmapIterator {
        BitmapIterator {
            bitmap: &self.bitmap,
            idx: 0,
        }
    }

    pub fn get(&self, idx: usize) -> bool {
        self.bitmap[idx / 64] & (1 << (idx % 64)) != 0
    }

    pub fn set(&mut self, idx: usize, value: bool) {
        if value {
            self.bitmap[idx / 64] |= 1 << (idx % 64);
        } else {
            self.bitmap[idx / 64] &= !(1 << (idx % 64));
        }
    }

    pub fn move_bit_range_forward(&mut self, src_idx: usize, dst_idx: usize, count: usize) {
        if src_idx == dst_idx {
            return;
        }
        assert!(
            src_idx <= dst_idx,
            "src_idx(=0x{src_idx:x}) > dst_idx(=0x{dst_idx:x})"
        );

        // TOOD: optimize
        for i in (0..count).rev() {
            self.set(dst_idx + i, self.get(src_idx + i));
        }
    }

    pub fn zero_bit_range(&mut self, idx: usize, count: usize) {
        if count == 0 {
            return;
        }
        // idx=1 => 0b0001
        // idx=2 => 0b0011
        // idx=3 => 0b0111
        // ...
        let prefix_mask = (1 << (idx % 64)) - 1;
        // idx=1 => 0b..._1111_1111_1110
        // idx=2 => 0b..._1111_1111_1100
        // idx=3 => 0b..._1111_1111_1000
        // ...
        let suffix_mask = !((1u64 << ((idx + count) % 64)) - 1);
        if idx / 64 == (idx + count) / 64 {
            // prefix and suffix are on the same cell, do the operation in one go
            self.bitmap[idx / 64] &= prefix_mask | suffix_mask;
            return;
        }

        if idx % 64 != 0 {
            self.bitmap[idx / 64] &= prefix_mask;
        }
        for cell in idx.div_ceil(64)..((idx + count) / 64) {
            self.bitmap[cell] = 0;
        }
        if (idx + count) % 64 != 0 {
            self.bitmap[(idx + count) / 64] &= suffix_mask;
        }
    }
}
const PAGE_ALLOC_CELLS: usize = 1024;
pub const PAGE_ALLOC_PAGES: usize = PAGE_ALLOC_CELLS * 64;
pub static PAGE_ALLOC: IrqMutex<BitmapPageAlloc<PAGE_ALLOC_CELLS>> =
    IrqMutex::new(BitmapPageAlloc::new(0));

pub struct PageSlice {
    buf: *mut (),
    len: usize,
}

impl PageSlice {
    pub fn as_ptr(&self) -> *const () {
        self.buf
    }

    pub fn as_mut_ptr(&mut self) -> *mut () {
        self.buf
    }

    pub fn as_slice(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.as_ptr() as *const u8, self.len) }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.as_mut_ptr() as *mut u8, self.len) }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn zero(&mut self) {
        unsafe { write_bytes(self.as_ptr() as *mut u64, 0, PAGE_SIZE / 8) };
    }
}

impl Drop for PageSlice {
    fn drop(&mut self) {
        unsafe {
            // Overwrite the page with poison
            #[cfg(debug_assertions)]
            write_bytes(self.buf as *mut u8, 0xa1, self.len);

            // Free the pages
            #[allow(static_mut_refs)]
            PAGE_ALLOC.lock().free(self.buf as usize, self.len / 4096);
        }
    }
}

pub struct BitmapPageAlloc<const SIZE: usize> {
    bitmap_alloc: Bitmap<SIZE>,
    ram_base: usize,
}

impl<const SIZE: usize> BitmapPageAlloc<SIZE> {
    pub const fn new(ram_base: usize) -> Self {
        Self {
            bitmap_alloc: Bitmap::new(),
            ram_base,
        }
    }

    pub fn alloc(&mut self, page_count: usize) -> Option<PageSlice> {
        if let Some(page_num) = self.bitmap_alloc.alloc(page_count) {
            #[cfg(feature = "log_alloc")]
            println!(
                "alloc: 0x{:x} - pages: {page_count}",
                self.ram_base + page_num * PAGE_SIZE
            );
            Some(PageSlice {
                buf: (self.ram_base + page_num * PAGE_SIZE) as *mut (),
                len: page_count * PAGE_SIZE,
            })
        } else {
            None
        }
    }

    pub fn alloc_zeroed(&mut self, page_count: usize) -> Option<PageSlice> {
        let mut slice = self.alloc(page_count);
        if let Some(slice) = &mut slice {
            slice.zero();
        }
        slice
    }

    pub fn mark_allocated(&mut self, addr: usize, page_count: usize) {
        debug_assert!(addr % PAGE_SIZE == 0, "addr must be page-aligned");
        debug_assert!(addr >= self.ram_base, "addr was before RAM");
        debug_assert!(
            addr + page_count * PAGE_SIZE
                <= self.ram_base + self.bitmap_alloc.bit_capacity() * PAGE_SIZE,
            "(addr + count) was after RAM"
        );
        self.bitmap_alloc
            .mark_allocated((addr - self.ram_base) / 4096, page_count);
    }

    pub fn free(&mut self, addr: usize, page_count: usize) {
        #[cfg(feature = "log_alloc")]
        println!("free: 0x{addr:x} - pages: {page_count}");
        debug_assert!(addr % PAGE_SIZE == 0, "addr must be page-aligned");
        debug_assert!(addr >= self.ram_base, "addr was before RAM");
        debug_assert!(
            addr + page_count * PAGE_SIZE
                <= self.ram_base + self.bitmap_alloc.bit_capacity() * PAGE_SIZE,
            "(addr + count) was after RAM"
        );
        self.bitmap_alloc
            .free((addr - self.ram_base) / 4096, page_count);
    }

    #[allow(dead_code)]
    pub fn overwrite_free_pages(&self) {
        let mut counter = 0;
        print!("Cleaning RAM: ");
        for (i, bit) in self.bitmap_alloc.iter().enumerate() {
            if !bit {
                if counter % 2048 == 0 {
                    print!(".");
                }
                let addr = self.ram_base + i * PAGE_SIZE;
                unsafe { write_bytes(addr as *mut u64, 0xb4, PAGE_SIZE / 8) };
                counter += 1;
            }
        }
        println!();
    }

    pub unsafe fn rebase(&mut self, new_base: usize) {
        self.ram_base = new_base;
    }

    pub unsafe fn expand_to(&mut self, new_base: usize, new_len: usize, prev_len: usize) {
        assert_eq!(
            self.ram_base % PAGE_SIZE,
            0,
            "ram_base is not aligned: 0x{:x}",
            self.ram_base
        );
        assert_eq!(
            new_base % PAGE_SIZE,
            0,
            "new_base is not aligned: 0x{new_base:x}"
        );
        assert_eq!(
            new_len % PAGE_SIZE,
            0,
            "new_len is not aligned: 0x{prev_len:x}"
        );
        assert_eq!(
            prev_len % PAGE_SIZE,
            0,
            "prev_len is not aligned: 0x{prev_len:x}"
        );
        assert!(
            new_base <= self.ram_base,
            "new ram region must contain old ram region (new_base(=0x{new_base:x}) > ram_base(={:x}))",
            self.ram_base
        );
        assert!(
            new_base + new_len >= self.ram_base + prev_len,
            "new ram region must contain old ram region (new_base(=0x{new_base:x}) + new_len(=0x{new_len:x}) >= self.ram_base(=0x{:x}) + prev_len(=0x{prev_len:x}))",
            self.ram_base
        );

        // Move existing bits to their new places, free the rest of the pages
        let addr_delta = self.ram_base - new_base;
        self.bitmap_alloc
            .move_bit_range_forward(0, addr_delta / PAGE_SIZE, prev_len / PAGE_SIZE);
        self.bitmap_alloc.zero_bit_range(0, addr_delta / PAGE_SIZE);
        self.bitmap_alloc.zero_bit_range(
            (addr_delta + prev_len) / PAGE_SIZE,
            (new_len - prev_len - addr_delta) / PAGE_SIZE,
        );
        self.rebase(new_base);
    }
}

pub fn alloc(page_count: usize) -> PageSlice {
    PAGE_ALLOC.lock().alloc(page_count).expect("OOM")
}

pub struct PageBox<T> {
    slice: PageSlice,
    _phantom_data: PhantomData<T>,
}

impl<T> PageBox<T> {
    pub fn new(value: T) -> Self {
        let page_count = size_of::<T>().div_ceil(PAGE_SIZE);
        let mut slice = alloc(page_count);
        unsafe {
            (slice.as_mut_ptr() as *mut T).write(value);
        }
        Self {
            slice,
            _phantom_data: PhantomData,
        }
    }

    pub fn as_ref(&self) -> &T {
        unsafe { &*(self.slice.as_ptr() as *const T) }
    }

    pub fn as_mut(&mut self) -> &mut T {
        unsafe { &mut *(self.slice.as_mut_ptr() as *mut T) }
    }

    pub fn into_inner(self) -> T {
        unsafe { (self.slice.as_ptr() as *mut T).read() }
    }

    pub fn leak(mut b: Self) -> &'static mut T {
        let ptr = b.slice.as_mut_ptr() as *mut T;
        mem::forget(b);
        unsafe { &mut *ptr }
    }
}

impl<T: FromZeros> PageBox<T> {
    pub fn new_zeroed() -> Self {
        let page_count = size_of::<T>().div_ceil(PAGE_SIZE);
        let slice = PAGE_ALLOC.lock().alloc_zeroed(page_count).expect("OOM");
        Self {
            slice,
            _phantom_data: PhantomData,
        }
    }
}

impl<T> Deref for PageBox<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl<T> DerefMut for PageBox<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut()
    }
}

impl<T> Drop for PageBox<T> {
    fn drop(&mut self) {
        unsafe { drop_in_place(self.slice.as_mut_ptr() as *mut T) }
    }
}

const EARLY_HEAP_SIZE: usize = 1024 * 1024;

struct EarlyHeap {
    _align: Align<PAGE_SIZE>,
    #[allow(dead_code)]
    data: [u8; EARLY_HEAP_SIZE],
}

static mut EARLY_HEAP: EarlyHeap = EarlyHeap {
    _align: Align::NEW,
    data: [0; EARLY_HEAP_SIZE],
};

pub unsafe fn init_early_heap() {
    let mut page_alloc = PAGE_ALLOC.lock();
    let heap_base = &raw const EARLY_HEAP as usize;
    unsafe { page_alloc.rebase(heap_base) };
    page_alloc.mark_allocated(heap_base, PAGE_ALLOC_PAGES);
    page_alloc.free(heap_base, EARLY_HEAP_SIZE / PAGE_SIZE);
}

pub fn add_memory_node(phy_addr: PhyAddr, len: usize) {
    let mut page_alloc = PAGE_ALLOC.lock();
    unsafe {
        page_alloc.expand_to(phy_addr.virt::<()>() as usize, len, EARLY_HEAP_SIZE);
        extern "C" {
            static _text_start: u8;
            static _end: u8;
        }
        let kernel_region = (&raw const _text_start as usize, &raw const _end as usize);
        let early_heap_region = (
            &raw const EARLY_HEAP as usize,
            &raw const EARLY_HEAP as usize + EARLY_HEAP_SIZE,
        );
        assert!(early_heap_region.0 >= kernel_region.0);
        assert!(early_heap_region.1 <= kernel_region.1);
        page_alloc.mark_allocated(
            kernel_region.0,
            (early_heap_region.0 - kernel_region.0) / PAGE_SIZE,
        );
        page_alloc.mark_allocated(
            early_heap_region.1,
            (kernel_region.1 - early_heap_region.0) / PAGE_SIZE,
        );
    }
}
