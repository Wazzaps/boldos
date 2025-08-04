#![no_std]
#![no_main]

use crate::aarch64::mmu::eject_lowmem;
use crate::page_alloc::PhyAddr;
use aarch64::{mmu, usermode};
use aarch64_cpu::registers::CurrentEL;
use core::arch::asm;
use core::fmt::Write;
use core::panic::PanicInfo;
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

#[no_mangle]
pub unsafe extern "C" fn kmain() -> ! {
    eject_lowmem();
    println!("--- BoldOS ---");
    println!("alloc: Initializing early allocator");
    page_alloc::init_early_heap();
    usermode::start();
    println!("Sleeping forever");
    loop {
        unsafe { asm!("wfi") }
    }
}

#[panic_handler]
fn rust_panic(info: &PanicInfo) -> ! {
    println!("[PANIC]: {}", info.message());
    if let Some(location) = info.location() {
        println!("location: {}", location);
    }

    loop {
        unsafe { asm!("wfi") }
    }
}
