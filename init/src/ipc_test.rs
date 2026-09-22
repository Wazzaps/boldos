use core::arch::asm;

use kernel_api::{KError, Syscall};
use num_enum::FromPrimitive;

use crate::{println, utils::sleep_sec};

#[derive(Debug)]
pub struct Handle(u64);

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            handle_close(self.0).expect("Failed to close handle");
        }
    }
}

pub fn handle_duplicate(handle: Handle) -> Result<Handle, KError> {
    let mut new_handle: u64;
    unsafe {
        asm!(
        "svc #0",
        in("x0") handle.0,
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

pub unsafe fn handle_close(handle: u64) -> Result<(), KError> {
    let mut result: i64;
    unsafe {
        asm!(
        "svc #0",
        in("x0") handle,
        in("x8") Syscall::HandleClose as u64,
        lateout("x0") result,
        );
    }
    if (result as i64) < 0 {
        Err(KError::from_primitive(result as i32))
    } else {
        Ok(())
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

pub fn ipc_test() -> ! {
    println!("ipc_test: Starting");

    let (_rx, _tx) = port_create().expect("Failed to create port");
    println!("ipc_test: Port created: {:?}, {:?}", _rx, _tx);

    let (_rx, _tx) = port_create().expect("Failed to create port");
    println!("ipc_test: Port created: {:?}, {:?}", _rx, _tx);

    loop {
        sleep_sec(1);
    }
}
