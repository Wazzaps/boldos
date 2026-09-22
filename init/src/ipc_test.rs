use core::arch::asm;

use kernel_api::{KError, Syscall};
use num_enum::FromPrimitive;

use crate::{
    println,
    utils::{sleep_sec, AsciiStr},
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

pub fn ipc_test() -> ! {
    println!("ipc_test: Starting");

    let (rx, tx) = port_create().expect("Failed to create port");
    println!("ipc_test: Port created: {:?}, {:?}", rx, tx);

    port_send(&tx, 0, b"Hello, world!", &[]).expect("Failed to send message");
    println!("ipc_test: First send went OK");

    let second_send = port_send(&tx, 0, b"Hello, world!", &[]);
    println!("ipc_test: Second send: {:?}", second_send);

    let mut buf = [0u8; 128];
    let (num_bytes, num_handles) =
        port_recv(&rx, 0, &mut buf, &mut []).expect("Failed to receive message");
    println!(
        "ipc_test: Received {num_bytes} bytes: '{}' + {num_handles} handles",
        AsciiStr(&buf[..num_bytes]),
    );
    let second_recv = port_recv(&rx, 0, &mut buf, &mut []);
    println!("ipc_test: Second receive: {:?}", second_recv);

    loop {
        sleep_sec(1);
    }
}
