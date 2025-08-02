use crate::aarch64::usermode::handle_syscall;
use crate::{print, println};
use aarch64_cpu::registers::{SPSel, ESR_EL1, FAR_EL1};
use core::arch::asm;
use tock_registers::interfaces::Readable;

#[no_mangle]
pub unsafe fn exception_handler(etype: u64, esr: u64, elr: u64, spsr: u64, far: u64) -> ! {
    println!("Exception SPSel: {}", SPSel.read(SPSel::SP));
    panic!(
        "Exception:\netype=0x{:x} esr=0x{:x} elr=0x{:x} spsr=0x{:x} far=0x{:x}",
        etype, esr, elr, spsr, far
    );
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct ExceptionContext {
    /// General Purpose Registers.
    pub gpr: [u64; 30],

    /// The link register, aka x30.
    pub lr: u64,

    /// Exception link register. The program counter at the time the exception happened.
    pub pc: u64,

    /// Saved program status.
    pub spsr: u64,

    /// The stack pointer.
    pub sp: u64,
}

#[no_mangle]
pub unsafe extern "C" fn exception_handler2(e: &mut ExceptionContext) {
    if ESR_EL1.get() == 0x56000000 {
        handle_syscall(e);
        return;
    }

    println!("-------------------------------------------");
    // let sp = (e as *const ExceptionContext as *const u8)
    //     .offset(size_of::<ExceptionContext>() as isize) as *const u64;
    println!("Registers:");
    for reg in e.gpr {
        print!("{:016x} ", reg);
    }
    println!();
    println!("Exception reason: 0x{:x}", ESR_EL1.get());
    println!("FAR (Address accessed): 0x{:x}", FAR_EL1.get());
    println!("PC: 0x{:x}", e.pc);
    println!("LR: 0x{:x}", e.lr);
    println!("SP: 0x{:016x}", e.sp);
    println!("SPSR: 0x{:x}", e.spsr);
    println!("-------------------------------------------");
    // print_stacktrace(e);

    loop {
        asm!("wfi");
    }
}

#[no_mangle]
pub unsafe extern "C" fn irq_handler(_e: &mut ExceptionContext) {
    // crate::arch::aarch64::interrupts::handle_irq(e);
    todo!()
}
