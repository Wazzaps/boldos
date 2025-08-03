use core::mem::size_of;

static mut UART0_ADDR: *mut u8 = 0x9000000 as *mut u8;

pub fn putc(ch: u8) {
    unsafe {
        UART0_ADDR.write_volatile(ch);
    }
}

pub fn puts(s: &[u8]) {
    for &ch in s {
        putc(ch);
    }
}

pub(crate) struct FmtWriteAdapter;

impl core::fmt::Write for FmtWriteAdapter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        puts(s.as_bytes());
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
    ($($arg:tt)*) => ($crate::drv::qemu_console::_print(format_args!($($arg)*)));
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

pub fn eject_lowmem() {
    unsafe {
        let new_value = (UART0_ADDR as u64) | 0xffff_ff00_0000_0000;
        UART0_ADDR = new_value as *mut u8;
    }
}
