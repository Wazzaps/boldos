use crate::aarch64::mmu;
use crate::aarch64::mmu::PageTable;
use crate::page_alloc::{PageBox, PhyAddr, PAGE_ALLOC, PAGE_SIZE};
use crate::println;
use aarch64_cpu::registers::{ELR_EL1, SPSR_EL1, SP_EL0, TTBR0_EL1};
use core::arch::asm;
use tock_registers::interfaces::Writeable;
use zerocopy::FromZeros;

#[derive(FromZeros)]
struct Thread {
    /// Virtual memory mapping
    page_table: PageTable,
    /// Stack contents
    stack: [u64; 1024], // 8KiB stack
    /// General-purpose registers
    gprs: [u64; 31],
    /// Link register
    lr: u64,
    /// Program counter
    pc: u64,
    /// Stack pointer
    sp: u64,
    /// Saved program status register
    spsr: u64,
}

impl Thread {
    pub unsafe fn enter(&mut self) -> ! {
        TTBR0_EL1.set_baddr(PhyAddr::from_virt(&raw const self.page_table).0 as u64);
        SPSR_EL1.set(self.spsr);
        SP_EL0.set(self.sp);
        ELR_EL1.set(self.pc);
        asm!("eret", options(noreturn))
    }
}

const DEFAULT_PC: u64 = 0x10000000;
const DEFAULT_SP: u64 = 0x8000000;
const DEFAULT_STACK_SIZE: u64 = 0x4000;

impl Thread {
    fn init(&mut self) {
        self.zero();
        self.pc = DEFAULT_PC;
        self.sp = DEFAULT_SP;
        self.spsr = 0x140;
    }
}

static INIT_BIN: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/init.bin"));

pub unsafe fn start() {
    println!(" user: Starting usermode");

    let mut code_slice = PAGE_ALLOC
        .lock()
        .alloc_zeroed(INIT_BIN.len().div_ceil(PAGE_SIZE))
        .expect("OOM");
    code_slice.as_mut_slice()[..INIT_BIN.len()].copy_from_slice(INIT_BIN);

    let mut thread = PageBox::<Thread>::new_zeroed();
    thread.init();

    const PAGE_FLAGS: u64 = mmu::PT_RW_EL0 | // non-privileged
        mmu::PT_ISH | // inner shareable
        mmu::PT_MEM; // normal memory
    for code_page in 0..INIT_BIN.len().div_ceil(PAGE_SIZE) {
        thread.page_table.vmap(
            DEFAULT_PC as usize + code_page * PAGE_SIZE,
            PhyAddr::from_virt(
                code_slice
                    .as_ptr()
                    .byte_offset((code_page * PAGE_SIZE) as isize),
            ),
            PAGE_FLAGS,
        );
    }
    for stack_page in (0..DEFAULT_STACK_SIZE).step_by(PAGE_SIZE) {
        let phy_addr = PhyAddr::from_virt(thread.stack.as_ptr().byte_offset(stack_page as isize));
        thread.page_table.vmap(
            (DEFAULT_SP - DEFAULT_STACK_SIZE + stack_page) as usize,
            phy_addr,
            PAGE_FLAGS,
        );
    }

    thread.enter();
}
