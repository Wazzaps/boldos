#![no_std]
#![no_main]

use core::{arch::asm, panic::PanicInfo};

const SYS_LOG: u64 = 0x00;

unsafe fn log_buf(s: &[u8]) {
    unsafe {
        asm!(
            "svc #0",
            in("x0") s.as_ptr() as u64,
            in("x1") s.len() as u64,
            in("x8") SYS_LOG,
        );
    }
}

#[no_mangle]
#[link_section = ".text.init"]
pub extern "C" fn _start() -> ! {
    unsafe {
        for _ in 0..3 {
            log_buf(b"Hello, world!");
        }
        log_buf(b"Hello, world 2!");
    }
    loop {}
}

#[panic_handler]
fn rust_panic(_info: &PanicInfo) -> ! {
    loop {}
}
