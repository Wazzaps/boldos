#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "alloc")]
use alloc::boxed::Box;

use crate::{
    ControlThreadOp, CreateThreadFlags, FutexOp, HandleId, KError, MemMapFlags, PhyMapFlags, Pid,
    RegionArg, RegionMapFlags, Syscall, kernel_device,
};
use core::arch::asm;
use core::fmt::Write;
use core::num::NonZeroU64;
use core::panic::PanicInfo;
use core::time::Duration;
use num_enum::FromPrimitive;

pub struct FmtWriteAdapter;

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
    ($($arg:tt)*) => ($crate::user::_print(format_args!($($arg)*)));
}

/// Like the `println!` macro in the standard library, but prints to the UART.
#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!(" {}: {}\n", env!("CARGO_CRATE_NAME"), format_args!($($arg)*)));
}

#[derive(Debug)]
pub struct Handle(NonZeroU64);

impl Handle {
    pub fn as_raw(&self) -> NonZeroU64 {
        self.0
    }

    pub fn as_u64(&self) -> u64 {
        self.0.get()
    }

    pub fn from_raw(raw: NonZeroU64) -> Self {
        Handle(raw)
    }

    pub fn from_u64(raw: u64) -> Self {
        Handle(NonZeroU64::new(raw).expect("Invalid handle"))
    }

    pub fn duplicate(&self) -> Result<Self, KError> {
        let mut new_handle: u64;
        unsafe {
            asm!(
            "svc #0",
            in("x0") self.0.get(),
            in("x8") Syscall::HandleDuplicate as u64,
            lateout("x0") new_handle,
            );
        }
        if (new_handle as i64) < 0 {
            Err(KError::from_primitive(new_handle as i32))
        } else {
            Ok(Handle(new_handle.try_into().unwrap()))
        }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        let mut result: i64;
        unsafe {
            asm!(
            "svc #0",
            in("x0") self.0.get(),
            in("x8") Syscall::HandleClose as u64,
            lateout("x0") result,
            );
        }
        if (result as i64) < 0 {
            panic!(
                "Failed to close handle: {:?}",
                KError::from_primitive(result as i32)
            );
        }
    }
}

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

pub fn sleep(duration: Duration) {
    unsafe {
        asm!(
        "svc #0",
        in("x0") duration.as_millis() as u64,
        in("x8") Syscall::SleepMs as u64,
        );
    }
}

pub fn delay_ticks(ticks: u64) {
    for _ in 0..ticks {
        unsafe { asm!("nop") };
    }
}

#[cfg(feature = "alloc")]
pub fn create_thread<F: FnOnce() -> ! + Send + 'static>(func: F) -> Result<Pid, KError> {
    let data = Box::into_raw(Box::new(func)) as *mut ();

    extern "C" fn trampoline<F: FnOnce() -> !>(data: *mut ()) -> ! {
        let closure = unsafe { Box::from_raw(data as *mut F) };
        closure()
    }

    create_thread_raw(
        trampoline::<F> as *const () as usize,
        data as usize,
        CreateThreadFlags::empty(),
        None,
        None,
    )
}

pub fn create_thread_raw(
    func: usize,
    data: usize,
    flags: CreateThreadFlags,
    handles: Option<&[Handle]>,
    mm: Option<&Handle>,
) -> Result<Pid, KError> {
    let mut res: i64;
    unsafe {
        asm!(
        "svc #0",
        in("x0") func as u64,
        in("x1") data as u64,
        in("x2") flags.bits(),
        in("x3") handles.map_or(0, |h| h.as_ptr() as u64),
        in("x4") handles.map_or(0, |h| h.len() as u64),
        in("x5") mm.map_or(0, |h| h.0.get()),
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

pub fn port_create() -> Result<(Handle, Handle), KError> {
    let mut recv_handle: u64;
    let mut send_handle: u64;
    unsafe {
        asm!(
        "svc #0",
        in("x8") Syscall::PortCreate as u64,
        lateout("x0") recv_handle,
        lateout("x1") send_handle,
        );
    }
    if (recv_handle as i64) < 0 {
        Err(KError::from_primitive(recv_handle as i32))
    } else {
        Ok((
            Handle(recv_handle.try_into().unwrap()),
            Handle(send_handle.try_into().unwrap()),
        ))
    }
}

pub fn port_send(
    port: &Handle,
    flags: u32,
    bytes: &[u8],
    handles: &[Handle],
) -> Result<(), KError> {
    let mut result: i64;
    unsafe {
        asm!(
        "svc #0",
        in("x0") port.0.get(),
        in("x1") flags,
        in("x2") bytes.as_ptr(),
        in("x3") bytes.len(),
        in("x4") handles.as_ptr(),
        in("x5") handles.len(),
        in("x8") Syscall::PortSend as u64,
        lateout("x0") result,
        );
    }
    if result < 0 {
        Err(KError::from_primitive(result as i32))
    } else {
        Ok(())
    }
}

pub fn port_recv(
    port: &Handle,
    flags: u32,
    bytes: &mut [u8],
    handles: &mut [HandleId],
) -> Result<(usize, usize), KError> {
    let mut num_bytes: u64;
    let mut num_handles: u64;
    unsafe {
        asm!(
        "svc #0",
        in("x0") port.0.get(),
        in("x1") flags,
        in("x2") bytes.as_ptr(),
        in("x3") bytes.len(),
        in("x4") handles.as_ptr(),
        in("x5") handles.len(),
        in("x8") Syscall::PortRecv as u64,
        lateout("x0") num_bytes,
        lateout("x1") num_handles,
        );
    }
    if (num_bytes as i64) < 0 {
        Err(KError::from_primitive(num_bytes as i32))
    } else {
        Ok((num_bytes as usize, num_handles as usize))
    }
}

pub fn region_create_virtual(flags: u32, len: usize) -> Result<Handle, KError> {
    let mut handle: u64;
    unsafe {
        asm!(
        "svc #0",
        in("x0") flags as u64,
        in("x1") len as u64,
        in("x8") Syscall::RegionCreateVirtual as u64,
        lateout("x0") handle,
        );
    }
    if (handle as i64) < 0 {
        Err(KError::from_primitive(handle as i32))
    } else {
        Ok(Handle(handle.try_into().unwrap()))
    }
}

pub fn region_create_physical(flags: u32, phy_addr: usize, len: usize) -> Result<Handle, KError> {
    let mut handle: u64;
    unsafe {
        asm!(
        "svc #0",
        in("x0") flags as u64,
        in("x1") phy_addr as u64,
        in("x2") len as u64,
        in("x8") Syscall::RegionCreatePhysical as u64,
        lateout("x0") handle,
        );
    }
    if (handle as i64) < 0 {
        Err(KError::from_primitive(handle as i32))
    } else {
        Ok(Handle(handle.try_into().unwrap()))
    }
}

pub fn region_get_size(region: &Handle) -> Result<usize, KError> {
    let mut size: u64;
    unsafe {
        asm!(
        "svc #0",
        in("x0") region.0.get(),
        in("x8") Syscall::RegionGetSize as u64,
        lateout("x0") size,
        );
    }
    if (size as i64) < 0 {
        Err(KError::from_primitive(size as i32))
    } else {
        Ok(size as usize)
    }
}

pub fn region_read(region: &Handle, bytes: &mut [u8], offset: usize) -> Result<usize, KError> {
    let mut result: u64;
    unsafe {
        asm!(
        "svc #0",
        in("x0") region.0.get(),
        in("x1") bytes.as_ptr(),
        in("x2") bytes.len(),
        in("x3") offset as u64,
        in("x8") Syscall::RegionRead as u64,
        lateout("x0") result,
        );
    }
    if (result as i64) < 0 {
        Err(KError::from_primitive(result as i32))
    } else {
        Ok(result as usize)
    }
}

pub fn region_read_full(region: &Handle, bytes: &mut [u8], offset: usize) -> Result<(), KError> {
    let read_bytes = region_read(region, bytes, offset)?;
    assert!(read_bytes <= bytes.len());
    if read_bytes < bytes.len() {
        return Err(KError::TooSmall);
    }
    Ok(())
}

pub fn region_write(region: &Handle, bytes: &[u8], offset: usize) -> Result<usize, KError> {
    let mut result: u64;
    unsafe {
        asm!(
        "svc #0",
        in("x0") region.0.get(),
        in("x1") bytes.as_ptr(),
        in("x2") bytes.len(),
        in("x3") offset as u64,
        in("x8") Syscall::RegionWrite as u64,
        lateout("x0") result,
        );
    }
    if (result as i64) < 0 {
        Err(KError::from_primitive(result as i32))
    } else {
        Ok(result as usize)
    }
}

pub fn region_write_full(region: &Handle, bytes: &[u8], offset: usize) -> Result<(), KError> {
    let written_bytes = region_write(region, bytes, offset)?;
    assert!(written_bytes <= bytes.len());
    if written_bytes < bytes.len() {
        return Err(KError::TooSmall);
    }
    Ok(())
}

pub fn mm_create(regions: &mut [RegionArg]) -> Result<Handle, KError> {
    let mut result: u64;
    unsafe {
        asm!(
        "svc #0",
        in("x0") 0,
        in("x1") regions.as_ptr(),
        in("x2") regions.len(),
        in("x8") Syscall::MmCreate as u64,
        lateout("x0") result,
        );
    }
    if (result as i64) < 0 {
        Err(KError::from_primitive(result as i32))
    } else {
        Ok(Handle(result.try_into().unwrap()))
    }
}

pub fn mm_modify(mm: Option<&Handle>, regions: &mut [RegionArg]) -> Result<(), KError> {
    let mut result: i64;
    unsafe {
        asm!(
        "svc #0",
        in("x0") mm.map_or(0, |h| h.0.get()),
        in("x1") regions.as_ptr(),
        in("x2") regions.len(),
        in("x8") Syscall::MmModify as u64,
        lateout("x0") result,
        );
    }
    if (result as i64) < 0 {
        Err(KError::from_primitive(result as i32))
    } else {
        Ok(())
    }
}

pub fn mm_map(mm: Option<&Handle>, region: &Handle, size: usize) -> Result<*mut (), KError> {
    let mut region_args = [RegionArg {
        region: region.0.get(),
        offset: 0,
        size: size,
        addr: 0,
        flags: RegionMapFlags::ReadWrite.bits(),
        #[cfg(target_pointer_width = "64")]
        _padding: 0,
    }];
    mm_modify(mm, &mut region_args)?;
    Ok(region_args[0].addr as *mut ())
}

pub fn mm_unmap_range(mm: Option<&Handle>, addr: *mut (), size: usize) -> Result<(), KError> {
    let mut region_args = [RegionArg {
        region: 0,
        offset: 0,
        size: size,
        addr: addr as usize,
        flags: 0,
        #[cfg(target_pointer_width = "64")]
        _padding: 0,
    }];
    mm_modify(mm, &mut region_args)
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

#[panic_handler]
fn rust_panic(info: &PanicInfo) -> ! {
    let _ = write!(FmtWriteAdapter, "Panic: {}\n", info.message());
    if let Some(location) = info.location() {
        let _ = write!(FmtWriteAdapter, "location: {}\n", location);
    }
    // unsafe { exit(0xffffffff) };
    loop {}
}
