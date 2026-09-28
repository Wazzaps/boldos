use core::{arch::asm, time::Duration};

use alloc::format;
use kernel_api::{ControlThreadOp, CreateThreadFlags, KError, RegionArg, Syscall};
use num_enum::FromPrimitive;

use crate::{
    println,
    utils::{control_thread, create_thread, sleep, AsciiStr},
};

#[derive(Debug)]
pub struct Handle(u64);

impl Handle {
    pub fn duplicate(&self) -> Result<Self, KError> {
        let mut new_handle: u64;
        unsafe {
            asm!(
            "svc #0",
            in("x0") self.0,
            in("x8") Syscall::HandleDuplicate as u64,
            lateout("x0") new_handle,
            );
        }
        if (new_handle as i64) < 0 {
            Err(KError::from_primitive(new_handle as i32))
        } else {
            assert_ne!(new_handle, 0);
            Ok(Handle(new_handle))
        }
    }

    pub fn close(&mut self) -> Result<(), KError> {
        if self.0 == 0 {
            return Ok(());
        }

        let mut result: i64;
        unsafe {
            asm!(
            "svc #0",
            in("x0") self.0,
            in("x8") Syscall::HandleClose as u64,
            lateout("x0") result,
            );
        }
        if (result as i64) < 0 {
            Err(KError::from_primitive(result as i32))
        } else {
            self.0 = 0;
            Ok(())
        }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.close().expect("Failed to close handle");
    }
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
        assert_ne!(recv_handle, 0);
        assert_ne!(send_handle, 0);
        Ok((Handle(recv_handle), Handle(send_handle)))
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
        in("x0") port.0,
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
    handles: &mut [Handle],
) -> Result<(usize, usize), KError> {
    let mut num_bytes: u64;
    let mut num_handles: u64;
    unsafe {
        asm!(
        "svc #0",
        in("x0") port.0,
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
        assert_ne!(handle, 0);
        Ok(Handle(handle))
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
        assert_ne!(handle, 0);
        Ok(Handle(handle))
    }
}

pub fn region_get_size(region: &Handle) -> Result<usize, KError> {
    let mut size: u64;
    unsafe {
        asm!(
        "svc #0",
        in("x0") region.0,
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
        in("x0") region.0,
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
        in("x0") region.0,
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

pub fn mm_modify(mm: Option<&Handle>, regions: &mut [RegionArg]) -> Result<(), KError> {
    let mut result: i64;
    unsafe {
        asm!(
        "svc #0",
        in("x0") mm.map(|h| h.0).unwrap_or(0),
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

pub fn mm_map(mm: Option<&Handle>, range: &Handle, size: usize) -> Result<*mut (), KError> {
    let mut region_args = [RegionArg {
        region: range.0,
        offset: 0,
        size: size,
        addr: 0,
        flags: 0,
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

pub fn ipc_test() -> ! {
    println!("ipc_test: Starting");

    // let (rx, tx) = port_create().expect("Failed to create port");
    // println!("ipc_test: Port created: {:?}, {:?}", rx, tx);

    // let pid = create_thread(
    //     move || {
    //         sleep(Duration::from_millis(1100));
    //         let mut i = 0u64;
    //         loop {
    //             match port_send(&tx, 0, format!("Hello, world! {i}").as_bytes(), &[]) {
    //                 Ok(_) => println!("ipc_test: Send went OK"),
    //                 Err(KError::PortFull) => println!("ipc_test: Port full"),
    //                 Err(e) => panic!("ipc_test: Unexpected error: {:?}", e),
    //             }
    //             i += 1;
    //             sleep(Duration::from_millis(1234));
    //         }
    //     },
    //     CreateThreadFlags::ShareHandles | CreateThreadFlags::SharePageTable,
    // )
    // .expect("Failed to create thread");
    // println!("ipc_test: Thread created: {}", pid);
    // control_thread(pid, ControlThreadOp::Resume).expect("Failed to resume thread");

    // let mut buf = [0u8; 128];
    // loop {
    //     match port_recv(&rx, 0, &mut buf, &mut []) {
    //         Ok((num_bytes, num_handles)) => {
    //             println!(
    //                 "ipc_test: Received {num_bytes} bytes: '{}' + {num_handles} handles",
    //                 AsciiStr(&buf[..num_bytes]),
    //             );
    //         }
    //         Err(KError::PortEmpty) => println!("ipc_test: Port empty"),
    //         Err(e) => panic!("ipc_test: Unexpected error: {:?}", e),
    //     }
    //     sleep(Duration::from_millis(500));
    // }

    let region = region_create_virtual(0, 8192).expect("Failed to create region");
    println!("ipc_test: Region created: {:?}", region);
    println!(
        "ipc_test: Region size: {:?}",
        region_get_size(&region).expect("Failed to get region size")
    );

    region_write_full(&region, b"hello ", 0).expect("Failed to write to region");
    region_write_full(&region, b"world", 6).expect("Failed to write to region");
    let mut buf = [0u8; 12];
    region_read_full(&region, &mut buf, 0).expect("Failed to read from region");
    println!("ipc_test: Region read: {}", AsciiStr(&buf));

    let mapped_ptr = mm_map(None, &region, 8192).expect("Failed to map region");
    println!("ipc_test: mapped region to {mapped_ptr:?}");
    {
        let mapped_region = unsafe { core::slice::from_raw_parts(mapped_ptr as *mut u8, 8192) };
        println!(
            "ipc_test: Region read via map: {}",
            AsciiStr(&mapped_region[..12])
        );
    }
    mm_unmap_range(None, mapped_ptr, 4096).expect("Failed to unmap mapped region");
    drop(region);

    let region = region_create_physical(0, 0x40000000, 4096).expect("Failed to create region");
    println!("ipc_test: Region created: {:?}", region);
    println!(
        "ipc_test: Region size: {:?}",
        region_get_size(&region).expect("Failed to get region size")
    );
    let mut buf = [0u8; 4];
    region_read_full(&region, &mut buf, 0).expect("Failed to read from region");
    println!("ipc_test: Region read: {:02x?}", buf);
    let mapped_ptr = mm_map(None, &region, 4096).expect("Failed to map region");
    println!("ipc_test: mapped region to {mapped_ptr:?}");
    {
        let mapped_region = unsafe { core::slice::from_raw_parts(mapped_ptr as *mut u8, 4096) };
        println!(
            "ipc_test: Region read via map: {:02x?}",
            &mapped_region[..4]
        );
    }
    mm_unmap_range(None, mapped_ptr, 4096).expect("Failed to unmap mapped region");
    // this will crash
    // println!(
    //     "ipc_test: Region read via map 2: {:02x?}",
    //     &mapped_region[..4]
    // );

    drop(region);

    loop {
        sleep(Duration::from_secs(9999));
    }
}
