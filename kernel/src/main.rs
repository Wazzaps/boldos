#![no_std]
#![no_main]

use crate::aarch64::mmu::eject_lowmem;
use crate::page_alloc::{PhyAddr, PAGE_ALLOC, PAGE_ALLOC_PAGES, PAGE_SIZE};
use aarch64::{mmu, usermode};
use aarch64_cpu::registers::CurrentEL;
use core::arch::asm;
use core::fmt::Write;
use core::panic::PanicInfo;
use elain::Align;
use tock_registers::interfaces::Readable;

pub mod aarch64;
mod drv;
pub mod page_alloc;

type InitFn = unsafe extern "C" fn() -> !;

/// # Safety
///
/// This function assumes it runs only once by `_start`
///
/// Do not print in this function, it will crash
#[no_mangle]
pub unsafe extern "C" fn kmain_nommu() -> ! {
    let curr_el = CurrentEL.read(CurrentEL::EL);
    assert_eq!(curr_el, 1, "Unexpectedly booted in EL{}", curr_el);

    mmu::init();

    /// Converts a low-mem function address to a high-mem address
    unsafe fn himem_func(f: InitFn) -> InitFn {
        unsafe { core::mem::transmute(PhyAddr(f as usize).virt::<()>()) }
    }
    himem_func(kmain)();
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

#[no_mangle]
pub unsafe extern "C" fn kmain() -> ! {
    eject_lowmem();
    println!("--- BoldOS ---");
    println!("alloc: Initializing early allocator");
    {
        let mut page_alloc = PAGE_ALLOC.lock();
        let heap_base = &raw const EARLY_HEAP as usize;
        unsafe { page_alloc.rebase(heap_base) };
        page_alloc.mark_allocated(heap_base, PAGE_ALLOC_PAGES);
        page_alloc.free(heap_base, EARLY_HEAP_SIZE / PAGE_SIZE);
    }
    usermode::start();
    println!("Sleeping forever");
    loop {
        unsafe { asm!("wfi") }
    }
}

#[panic_handler]
fn rust_panic(info: &PanicInfo) -> ! {
    print!("[PANIC]: ");
    let _ = write!(drv::qemu_console::FmtWriteAdapter, "{}", info.message());
    print!("\n");

    loop {
        unsafe { asm!("wfi") }
    }
}
