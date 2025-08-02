use core::arch::asm;
use kernel_api::{KError, PhyMapFlags, Syscall};
use num_enum::FromPrimitive;

pub unsafe fn log_buf(s: &[u8]) {
    unsafe {
        asm!(
        "svc #0",
        in("x0") s.as_ptr() as u64,
        in("x1") s.len() as u64,
        in("x8") Syscall::Log as u64,
        );
    }
}

pub unsafe fn phy_map(phy_addr: u64, len: u64, flags: PhyMapFlags) -> Result<*const (), KError> {
    let mut virt_addr: u64;
    unsafe {
        asm!(
        "svc #0",
        in("x0") phy_addr,
        in("x1") len,
        in("x2") flags.bits(),
        in("x8") Syscall::PhyMap as u64,
        lateout("x0") virt_addr,
        );
    }
    if (virt_addr as i64) < 0 {
        Err(KError::from_primitive(virt_addr as i32))
    } else {
        Ok(virt_addr as _)
    }
}

pub(crate) struct FmtWriteAdapter;

impl core::fmt::Write for FmtWriteAdapter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        unsafe { log_buf(s.as_bytes()) };
        Ok(())
    }
}

/// Prints the given formatted string to the UART.
#[doc(hidden)]
pub fn _print(args: core::fmt::Arguments) {
    use core::fmt::Write;

    let _ = FmtWriteAdapter.write_fmt(args);
}

/// Like the `print!` macro in the standard library, but prints to the UART.
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::utils::_print(format_args!($($arg)*)));
}

/// Like the `println!` macro in the standard library, but prints to the UART.
#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[allow(dead_code)]
pub fn dump_hex<T>(val: &T) {
    let size = size_of::<T>() as isize;
    let val = val as *const T as *const u8;
    for i in 0..size {
        unsafe {
            print!("{:02x}", *val.offset(i));
        }
        if i % 4 == 3 {
            print!(" ");
        }
        if i % 32 == 31 {
            println!();
        }
    }
    println!();
}

#[allow(dead_code)]
pub fn dump_hex_slice(val: &[u8]) {
    for (i, byte) in val.iter().enumerate() {
        print!("{:02x}", byte);
        if i % 4 == 3 {
            print!(" ");
        }
        if i % 32 == 31 {
            println!();
        }
    }
    println!();
}
