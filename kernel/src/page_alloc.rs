use crate::{print, println};
use core::ptr::write_bytes;

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
}
pub static mut PAGE_ALLOC: BitmapPageAlloc<1024> = BitmapPageAlloc::new(0);

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

    pub fn len(&self) -> usize {
        self.len
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
            PAGE_ALLOC.free(self.buf as usize, self.len / 4096);
        }
    }
}

pub struct BitmapPageAlloc<const SIZE: usize> {
    bitmap_alloc: Bitmap<SIZE>,
    ram_base: usize,
}

impl<const SIZE: usize> BitmapPageAlloc<SIZE> {
    const PAGE_SIZE: usize = 4096;
    pub const fn new(ram_base: usize) -> Self {
        Self {
            bitmap_alloc: Bitmap::new(),
            ram_base,
        }
    }

    pub fn alloc(&mut self, page_count: usize) -> Option<PageSlice> {
        if let Some(page_num) = self.bitmap_alloc.alloc(page_count) {
            Some(PageSlice {
                buf: (self.ram_base + page_num * Self::PAGE_SIZE) as *mut (),
                len: page_count * Self::PAGE_SIZE,
            })
        } else {
            None
        }
    }

    pub fn mark_allocated(&mut self, addr: usize, page_count: usize) {
        debug_assert!(addr % Self::PAGE_SIZE == 0, "addr must be page-aligned");
        debug_assert!(addr >= self.ram_base, "addr was before RAM");
        debug_assert!(
            addr + page_count * Self::PAGE_SIZE
                <= self.ram_base + self.bitmap_alloc.bit_capacity() * Self::PAGE_SIZE,
            "(addr + count) was after RAM"
        );
        self.bitmap_alloc
            .mark_allocated((addr - self.ram_base) / 4096, page_count);
    }

    pub fn free(&mut self, addr: usize, page_count: usize) {
        debug_assert!(addr % Self::PAGE_SIZE == 0, "addr must be page-aligned");
        debug_assert!(addr >= self.ram_base, "addr was before RAM");
        debug_assert!(
            addr + page_count * Self::PAGE_SIZE
                <= self.ram_base + self.bitmap_alloc.bit_capacity() * Self::PAGE_SIZE,
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
                let addr = self.ram_base + i * Self::PAGE_SIZE;
                unsafe { write_bytes(addr as *mut u64, 0xb4, Self::PAGE_SIZE / 8) };
                counter += 1;
            }
        }
        println!();
    }

    pub unsafe fn rebase(&mut self, new_base: usize) {
        self.ram_base = new_base;
    }
}

pub fn alloc(page_count: usize) -> PageSlice {
    unsafe {
        #[allow(static_mut_refs)]
        PAGE_ALLOC.alloc(page_count).unwrap()
    }
}

// #[cfg(test)]
// pub fn test() {
//     let mut bitmap = [
//         0xfffffffffffffffd,
//         0xfffffffffffffffd,
//         0xfffffffffffffff0,
//         0xfffffffffffffffd,
//     ];
//     let alloc = Bitmap::new(&mut bitmap);
//     println!("{:?}", alloc.find_hole(2));
// }
