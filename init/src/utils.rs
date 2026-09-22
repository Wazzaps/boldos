use alloc::boxed::Box;
use core::arch::asm;
use kernel_api::{
    kernel_device, ControlThreadOp, CreateThreadFlags, FutexOp, KError, MemMapFlags, PhyMapFlags,
    Pid, Syscall,
};
use num_enum::FromPrimitive;

pub unsafe fn exit(code: u32) -> ! {
    unsafe {
        asm!(
        "svc #0",
        in("x0") code as u64,
        in("x8") Syscall::Exit as u64,
        options(noreturn),
        )
    }
}

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

pub unsafe fn phy_map(
    phy_addr: usize,
    len: usize,
    flags: PhyMapFlags,
) -> Result<(*const (), u64), KError> {
    let mut virt_addr: u64;
    let mut out_phy_addr: u64;
    unsafe {
        asm!(
        "svc #0
        dmb ish",
        in("x0") phy_addr as u64,
        in("x1") len as u64,
        in("x2") flags.bits(),
        in("x8") Syscall::PhyMap as u64,
        lateout("x0") virt_addr,
        lateout("x1") out_phy_addr,
        );
    }
    if (virt_addr as i64) < 0 {
        Err(KError::from_primitive(virt_addr as i32))
    } else {
        Ok((virt_addr as _, out_phy_addr))
    }
}

pub unsafe fn mem_map(len: usize, flags: MemMapFlags) -> Result<*const (), KError> {
    let mut virt_addr: u64;
    unsafe {
        asm!(
        "dmb ish
        svc #0",
        in("x0") len as u64,
        in("x1") flags.bits(),
        in("x8") Syscall::MemMap as u64,
        lateout("x0") virt_addr,
        );
    }
    if (virt_addr as i64) < 0 {
        Err(KError::from_primitive(virt_addr as i32))
    } else {
        Ok(virt_addr as _)
    }
}

pub unsafe fn mem_unmap(virt_addr: *const (), len: usize) -> Result<(), KError> {
    let mut res: i64;
    unsafe {
        asm!(
        "dmb ish
        svc #0",
        in("x0") virt_addr as u64,
        in("x1") len as u64,
        in("x8") Syscall::MemUnmap as u64,
        lateout("x0") res,
        );
    }
    if res < 0 {
        Err(KError::from_primitive(res as i32))
    } else {
        Ok(())
    }
}

pub unsafe fn download_more_ram(phy_addr: usize, len: usize) -> Result<(), KError> {
    let mut res: i64;
    unsafe {
        asm!(
        "svc #0",
        in("x0") phy_addr as u64,
        in("x1") len as u64,
        in("x8") Syscall::DownloadMoreRam as u64,
        lateout("x0") res,
        );
    }
    if res < 0 {
        Err(KError::from_primitive(res as i32))
    } else {
        Ok(())
    }
}

pub unsafe fn load_kernel_device<T: kernel_device::KernelDeviceId + Sized>(
    req: &T,
) -> Result<(), KError> {
    let req_ptr = req as *const _;
    let req_len = size_of_val(req);
    let mut res: i64;
    unsafe {
        asm!(
        "svc #0",
        in("x0") req_ptr as u64,
        in("x1") req_len as u64,
        in("x2") T::ID as u64,
        in("x8") Syscall::LoadKernelDevice as u64,
        lateout("x0") res,
        );
    }
    if res < 0 {
        Err(KError::from_primitive(res as i32))
    } else {
        Ok(())
    }
}

pub fn sleep_sec(sec: u64) {
    unsafe {
        asm!(
        "svc #0",
        in("x0") sec as u64,
        in("x8") Syscall::SleepSec as u64,
        );
    }
}

pub fn delay_ticks(ticks: u64) {
    for _ in 0..ticks {
        unsafe { asm!("nop") };
    }
}

pub fn create_thread<F: FnOnce() -> ! + Send + 'static>(
    func: F,
    flags: CreateThreadFlags,
) -> Result<Pid, KError> {
    let data = Box::into_raw(Box::new(func)) as *mut ();

    extern "C" fn trampoline<F: FnOnce() -> !>(data: *mut ()) -> ! {
        let closure = unsafe { Box::from_raw(data as *mut F) };
        closure()
    }

    create_thread_raw(trampoline::<F>, data, flags)
}

pub fn create_thread_raw(
    func: extern "C" fn(*mut ()) -> !,
    data: *mut (),
    flags: CreateThreadFlags,
) -> Result<Pid, KError> {
    let mut res: i64;
    unsafe {
        asm!(
        "svc #0",
        in("x0") func as u64,
        in("x1") data as u64,
        in("x2") flags.bits(),
        in("x8") Syscall::CreateThread as u64,
        lateout("x0") res,
        );
    }
    if res < 0 {
        Err(KError::from_primitive(res as i32))
    } else {
        Ok(res as Pid)
    }
}

pub fn control_thread(pid: Pid, op: ControlThreadOp) -> Result<(), KError> {
    let mut res: i64;
    unsafe {
        asm!(
        "svc #0",
        in("x0") pid as u64,
        in("x1") op as u64,
        in("x8") Syscall::ControlThread as u64,
        lateout("x0") res,
        );
    }
    if res < 0 {
        Err(KError::from_primitive(res as i32))
    } else {
        Ok(())
    }
}

pub fn get_pid() -> Pid {
    let mut res: u64;
    unsafe {
        asm!(
        "svc #0",
        in("x8") Syscall::GetPid as u64,
        lateout("x0") res,
        );
    }
    res as Pid
}

pub fn virt_to_phys(virt_addr: *const ()) -> Result<u64, KError> {
    let mut res: i64;
    unsafe {
        asm!(
        "svc #0",
        in("x0") virt_addr as u64,
        in("x8") Syscall::VirtToPhys as u64,
        lateout("x0") res,
        );
    }
    if res < 0 {
        Err(KError::from_primitive(res as i32))
    } else {
        Ok(res as u64)
    }
}

pub fn futex(word: *const u32, op: FutexOp, val: u32, timeout_micros: u64) -> Result<(), KError> {
    let mut res: i64;
    unsafe {
        asm!(
        "svc #0",
        in("x0") word as u64,
        in("x1") op.bits(),
        in("x2") val as u64,
        in("x3") timeout_micros as u64,
        in("x8") Syscall::Futex as u64,
        lateout("x0") res,
        );
    }
    if res < 0 {
        Err(KError::from_primitive(res as i32))
    } else {
        Ok(())
    }
}

pub fn futex_wait(word: *const u32, val: u32, timeout_micros: u64) -> Result<(), KError> {
    futex(word, FutexOp::WAIT, val, timeout_micros)
}

pub fn futex_wake(word: *const u32, waiters_to_wake: u32) -> Result<(), KError> {
    futex(word, FutexOp::WAKE, waiters_to_wake, 0)
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
    ($($arg:tt)*) => ($crate::print!(" init: {}\n", format_args!($($arg)*)));
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

pub struct AsciiStr<'a>(pub &'a [u8]);

impl<'a> core::fmt::Display for AsciiStr<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for &ch in self.0 {
            if ch.is_ascii_graphic() || ch == b' ' || ch == b'\t' {
                write!(f, "{}", ch as char)?;
            } else {
                write!(f, "?")?;
            }
        }
        Ok(())
    }
}

#[macro_export]
macro_rules! set_msr_const {
    ($name: ident, $value: expr) => {
        ::core::arch::asm!(
            concat!("msr ", stringify!($name), ", {}"),
            const $value,
            options(nomem, nostack)
        )
    };
}

#[macro_export]
macro_rules! set_msr {
    ($name: ident, $value: expr) => {
        ::core::arch::asm!(
            concat!("msr ", stringify!($name), ", {}"),
            in(reg) $value,
            options(nomem, nostack)
        )
    };
}

#[macro_export]
macro_rules! get_msr {
    ($name: ident) => {{
        let val: u64;
        ::core::arch::asm!(
            concat!("mrs {:x}, ", stringify!($name)),
            out(reg) val,
            options(nomem, nostack)
        );
        val
    }};
}
