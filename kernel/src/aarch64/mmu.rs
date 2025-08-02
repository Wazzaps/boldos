use core::arch::asm;

use crate::page_alloc::{PageBox, PhyAddr, PAGE_SIZE};
use aarch64_cpu::registers::{ReadWriteable, Writeable, VBAR_EL1};
use aarch64_cpu::registers::{MAIR_EL1, SCTLR_EL1, TCR_EL1, TTBR0_EL1, TTBR1_EL1};
use tock_registers::interfaces::Readable;
use zerocopy::FromZeros;

#[allow(unused_imports)]
use crate::println;

pub const PT_PAGE: u64 = 0b11;
pub const PT_BLOCK: u64 = 0b01;
pub const PT_AF: u64 = 1 << 10;
pub const PT_RW_EL1: u64 = 0b00 << 6;
pub const PT_RW_EL0: u64 = 0b01 << 6;
pub const PT_RO_EL1: u64 = 0b10 << 6;
pub const PT_RO_EL0: u64 = 0b11 << 6;
pub const PT_ISH: u64 = 0b11 << 8;
pub const PT_MEM: u64 = 0 << 2;
pub const PT_DEV: u64 = 1 << 2;

#[repr(align(4096))]
#[derive(FromZeros)]
pub struct PageTable(pub [u64; 512]);

impl PageTable {
    pub const unsafe fn new() -> Self {
        Self([0; 512])
    }

    pub fn get(&self, idx: usize) -> PageGetResult {
        let raw = self.0[idx];
        match raw {
            0 => PageGetResult::Free,
            raw if raw & 0b11 == PT_BLOCK => PageGetResult::Block,
            raw => {
                let phy_addr = PhyAddr(raw as usize & 0x7FFFFFF000);
                PageGetResult::PageTable(unsafe { &*phy_addr.virt() })
            }
        }
    }

    pub fn get_mut(&mut self, idx: usize) -> PageGetMutResult {
        match self.0[idx] {
            0 => PageGetMutResult::Free,
            raw if raw & PT_BLOCK != 0 => PageGetMutResult::Block,
            raw => {
                let phy_addr = PhyAddr(raw as usize & 0x7FFFFFF000);
                PageGetMutResult::PageTable(unsafe { &mut *phy_addr.virt_mut() })
            }
        }
    }

    pub fn get_mut_or_alloc(&mut self, idx: usize, flags: u64) -> &mut Self {
        let raw = self.0[idx];
        if raw == 0 {
            // Allocate new page table
            // TODO: Doesn't handle oom
            let new_table = PageBox::leak(PageBox::<PageTable>::new_zeroed());
            let phy_addr = PhyAddr::from_virt(new_table);
            #[cfg(feature = "log_mmu")]
            println!("  mmu: Allocated PT at {:?}", phy_addr);
            self.0[idx] = phy_addr.0 as u64 | flags;
            new_table
        } else {
            // Return existing page table
            let phy_addr = PhyAddr(raw as usize & 0x7FFFFFF000);
            unsafe { &mut *phy_addr.virt_mut() }
        }
    }

    /// Maps a single physical page to a given virtual address
    ///
    /// # Safety
    ///
    /// Must be called on a L0 table
    pub fn vmap_at(&mut self, vaddr: usize, paddr: PhyAddr, attrs: u64) {
        const COMMON_FLAGS: u64 = PT_PAGE | // it has the "Present" flag, which must be set, and we have area in it mapped by pages
            PT_AF; // accessed flag. Without this we're going to have a Data Abort exception

        const TABLE_FLAGS: u64 = PT_PAGE | // it has the "Present" flag, which must be set, and we have area in it mapped by pages
            PT_AF | // accessed flag. Without this we're going to have a Data Abort exception
            PT_ISH | // inner shareable
            PT_MEM; // normal memory

        #[cfg(feature = "log_mmu")]
        println!("  mmu: Mapping {:?} to 0x{:x}", paddr, vaddr);
        assert_eq!(PAGE_SIZE, 4096); // TODO
        assert!(vaddr < 0x8000000000);
        assert_eq!(vaddr % PAGE_SIZE, 0);
        let l1 = self.get_mut_or_alloc(vaddr >> 39, TABLE_FLAGS);
        let l2 = l1.get_mut_or_alloc((vaddr >> 30) % 512, TABLE_FLAGS);
        let l3 = l2.get_mut_or_alloc((vaddr >> 21) % 512, TABLE_FLAGS);
        let entry = &mut l3.0[(vaddr >> 12) % 512];
        assert_eq!(*entry, 0, "Tried to map memory (vaddr={:?} paddr={:?}) that is already occupied with entry: 0x{:016x}", vaddr, paddr, *entry);
        *entry = paddr.0 as u64 | COMMON_FLAGS | attrs;
    }

    /// Maps a given physical region to an arbitrary free virtual region
    ///
    /// # Safety
    ///
    /// Must be called on a L0 table
    pub fn vmap(&mut self, paddr: PhyAddr, size_bytes: usize, attrs: u64) -> usize {
        assert_eq!(size_bytes % PAGE_SIZE, 0);
        let vaddr = self
            .find_hole(0x50000000, size_bytes)
            .expect("Out of virtual memory space");
        for offset in (0..size_bytes).step_by(PAGE_SIZE) {
            self.vmap_at(vaddr + offset, PhyAddr(paddr.0 + offset), attrs);
        }
        vaddr
    }

    /// Finds an unallocated virtual region of the given size, with a given minimum address
    pub fn find_hole(&self, start_vaddr: usize, size_bytes: usize) -> Option<usize> {
        let mut current_vaddr = start_vaddr;
        while current_vaddr < 0x8000000000 {
            match self.measure_contiguous_region(current_vaddr, usize::MAX, size_bytes) {
                ContiguousRegion::Allocated { len_bytes } => {
                    current_vaddr += len_bytes;
                }
                ContiguousRegion::Free { len_bytes } => {
                    if len_bytes >= size_bytes {
                        return Some(current_vaddr);
                    }
                    current_vaddr += len_bytes;
                }
            }
        }
        None
    }

    /// Measure the contiguous region of virtual memory that is either allocated or free, starting
    /// from the given virtual address.
    ///
    /// Stops when we've reached the end of virtual memory or the given size limit for the region type.
    ///
    /// # Safety
    ///
    /// Must be called on an L0 table
    pub fn measure_contiguous_region(
        &self,
        start_vaddr: usize,
        max_alloc_bytes: usize,
        max_free_bytes: usize,
    ) -> ContiguousRegion {
        self.measure_contiguous_region_helper(start_vaddr, max_alloc_bytes, max_free_bytes, 39)
    }

    fn measure_contiguous_region_helper(
        &self,
        start_vaddr: usize,
        max_alloc_bytes: usize,
        max_free_bytes: usize,
        addr_shift: usize,
    ) -> ContiguousRegion {
        assert!(addr_shift >= 21);
        assert!(addr_shift <= 39);
        let mut current_vaddr = start_vaddr;
        let mut current_len = 0;
        let mut is_allocated = false;
        let page_size = 1 << (addr_shift - 9);
        for idx in ((current_vaddr >> addr_shift) % 512)..512 {
            match self.get(idx) {
                PageGetResult::Block => {
                    if !is_allocated && current_len != 0 {
                        return ContiguousRegion::Free {
                            len_bytes: current_len,
                        };
                    }
                    is_allocated = true;
                }
                PageGetResult::Free => {
                    if is_allocated && current_len != 0 {
                        return ContiguousRegion::Allocated {
                            len_bytes: current_len,
                        };
                    }
                    is_allocated = false;
                }
                PageGetResult::PageTable(inner_table) => {
                    assert_ne!(addr_shift, 21, "Got an inner page table at a L2 table");
                    match inner_table.measure_contiguous_region_helper(
                        current_vaddr,
                        max_alloc_bytes,
                        max_free_bytes,
                        addr_shift - 9,
                    ) {
                        ContiguousRegion::Allocated { len_bytes } => {
                            if !is_allocated && current_len != 0 {
                                return ContiguousRegion::Free {
                                    len_bytes: current_len,
                                };
                            }
                            if len_bytes != page_size {
                                return ContiguousRegion::Allocated {
                                    len_bytes: current_len + len_bytes,
                                };
                            }
                            is_allocated = true;
                        }
                        ContiguousRegion::Free { len_bytes } => {
                            if is_allocated && current_len != 0 {
                                return ContiguousRegion::Allocated {
                                    len_bytes: current_len,
                                };
                            }
                            if len_bytes != page_size {
                                return ContiguousRegion::Free {
                                    len_bytes: current_len + len_bytes,
                                };
                            }
                            is_allocated = false;
                        }
                    }
                }
            }

            current_len += page_size;
            current_vaddr += page_size;
            if is_allocated && current_len >= max_alloc_bytes {
                return ContiguousRegion::Allocated {
                    len_bytes: current_len,
                };
            } else if !is_allocated && current_len >= max_free_bytes {
                return ContiguousRegion::Free {
                    len_bytes: current_len,
                };
            }
        }

        if is_allocated {
            ContiguousRegion::Allocated {
                len_bytes: current_len,
            }
        } else {
            ContiguousRegion::Free {
                len_bytes: current_len,
            }
        }
    }
}

pub enum ContiguousRegion {
    Allocated { len_bytes: usize },
    Free { len_bytes: usize },
}

pub enum PageGetResult<'a> {
    Free,
    PageTable(&'a PageTable),
    Block,
}

pub enum PageGetMutResult<'a> {
    Free,
    PageTable(&'a mut PageTable),
    Block,
}

static mut TABLE_L0: PageTable = PageTable([0; 512]);
static mut TABLE_L1_MEM: PageTable = PageTable([0; 512]);
static mut TABLE_L1_DEV: PageTable = PageTable([0; 512]);

unsafe fn make_page_table_l0(page_table: &mut PageTable) {
    page_table.0[0] = &raw const TABLE_L1_MEM as u64 | PT_PAGE;
    page_table.0[510] = &raw const TABLE_L1_MEM as u64 | PT_PAGE;
    page_table.0[511] = &raw const TABLE_L1_DEV as u64 | PT_PAGE;
}

unsafe fn make_page_table_l1(page_table: &mut PageTable, attr: u64) {
    for i in 0..512 {
        page_table.0[i] = ((i as u64) << 30) | // Physical address
            PT_BLOCK | // map 2M block
            PT_AF | // accessed flag
            PT_RW_EL1 | // R/W only by EL1
            PT_ISH | attr;
    }
}

pub unsafe fn init() {
    // Create identity-mapped page tables at the start of low mem (0x0..) and at the end of
    // high mem (0xffffff0000000000..), and again at the end of high mem but with nGnRnE attrs
    #[allow(static_mut_refs)]
    make_page_table_l0(&mut TABLE_L0);
    #[allow(static_mut_refs)]
    make_page_table_l1(&mut TABLE_L1_MEM, PT_MEM);
    #[allow(static_mut_refs)]
    make_page_table_l1(&mut TABLE_L1_DEV, PT_DEV);

    // Set memory attributes
    MAIR_EL1.write(
        // Attr 0 = Normal memory
        MAIR_EL1::Attr0_Normal_Outer::WriteBack_NonTransient_ReadWriteAlloc
            + MAIR_EL1::Attr0_Normal_Inner::WriteBack_NonTransient_ReadWriteAlloc
        // Attr 1 = Device memory
            + MAIR_EL1::Attr1_Device::nonGathering_nonReordering_noEarlyWriteAck,
    );

    // Set translation control register
    TCR_EL1.write(
        TCR_EL1::TG0::KiB_4
            + TCR_EL1::TG1::KiB_4
            + TCR_EL1::T0SZ.val(16)
            + TCR_EL1::T1SZ.val(16)
            + TCR_EL1::SH0::Inner
            + TCR_EL1::SH1::Inner
            + TCR_EL1::ORGN0::WriteBack_ReadAlloc_WriteAlloc_Cacheable
            + TCR_EL1::ORGN1::WriteBack_ReadAlloc_WriteAlloc_Cacheable,
    );

    // Set translation table base registers
    let table_l0_ptr = &raw const TABLE_L0 as u64;
    TTBR0_EL1.write(TTBR0_EL1::CnP::SET + TTBR0_EL1::BADDR.val(table_l0_ptr >> 1));
    TTBR1_EL1.write(TTBR1_EL1::CnP::SET + TTBR1_EL1::BADDR.val(table_l0_ptr >> 1));

    // Invalidate all translation tables
    asm!("dsb ish");
    asm!("tlbi vmalle1");
    asm!("dsb ish");
    asm!("isb");

    // Enable MMU and cache
    SCTLR_EL1.modify(SCTLR_EL1::M::SET + SCTLR_EL1::C::SET + SCTLR_EL1::I::SET);
    SCTLR_EL1.set(SCTLR_EL1.get() & !((1 << 57) | (1 << 23))); // disable EPAN and SPAN

    // Invalidate instruction cache
    asm!("isb");
}

pub unsafe fn eject_lowmem() {
    extern "C" {
        static _vectors: u8;
    }
    VBAR_EL1.set(&raw const _vectors as u64);
    // Change sp to high mem by ORR'ing it with 0xffff_ff00_0000_0000
    asm!("
        mov {1}, sp
        orr {1}, {1}, {0}
        mov sp, {1}
    ", in(reg) 0xffff_ff00_0000_0000u64, out(reg) _);

    // TODO: Disable the low-mem stack
    // TTBR0_EL1.set(0);
    // asm!("isb");
}
