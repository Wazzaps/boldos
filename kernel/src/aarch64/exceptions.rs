use crate::println;
use aarch64_cpu::registers::{SPSel, ESR_EL1};
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

// copy using the LDTR instruction
unsafe fn copy_from_user(user_pointer: usize, user_len: usize, target: &mut [u8]) {
    assert_eq!(user_len, target.len());
    for i in 0..user_len {
        let mut value: u32;
        asm!("ldtrb {0:w}, [{1}]", out(reg) value, in(reg) user_pointer + i);
        target[i] = value as u8;
    }
}

#[no_mangle]
pub unsafe extern "C" fn exception_handler2(e: &mut ExceptionContext) {
    if ESR_EL1.get() == 0x56000000 {
        let syscall_num = e.gpr[8];
        match syscall_num {
            0 => {
                let mut buf = [0u8; 128];
                let ptr = e.gpr[0];
                let len = e.gpr[1];
                copy_from_user(ptr as usize, len as usize, &mut buf[..len as usize]);
                {
                    println!("  log: {}", str::from_utf8(&buf[..len as usize]).unwrap());
                }
            }
            _ => {
                println!("Unknown syscall: {syscall_num}");
                e.gpr[0] = u64::MAX;
            }
        }
        return;
    }

    // println!("-------------------------------------------");
    // // let sp = (e as *const ExceptionContext as *const u8)
    // //     .offset(size_of::<ExceptionContext>() as isize) as *const u64;
    // println!("Registers:");
    // for reg in e.gpr {
    //     print!("{:016x} ", reg);
    // }
    // println!();
    // println!("Exception reason: 0x{:x}", get_msr!(esr_el1));
    // println!("FAR (Address accessed): 0x{:x}", get_msr!(far_el1));
    // println!("PC: 0x{:x}", e.pc);
    // println!("LR: 0x{:x}", e.lr);
    // println!("SP: 0x{:016x}", e.sp);
    // println!("SPSR: 0x{:x}", e.spsr);
    // println!("-------------------------------------------");
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
