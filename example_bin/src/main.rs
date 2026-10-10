#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec;
use kernel_api::println;
use kernel_api::user::{exit, port_recv, AsciiStr, Handle};

fn main(recv_handle: Handle) {
    println!(
        "Hello from example_bin! Here's a heap vector: {:?}",
        vec![1, 2, 3]
    );

    let mut buf: [u8; 128] = [0u8; 128];
    let (len, _) =
        port_recv(&recv_handle, 0, &mut buf, &mut []).expect("Failed to receive message");
    println!("Received message from parent: {}", AsciiStr(&buf[..len]));
}

#[no_mangle]
#[link_section = ".text.init"]
pub extern "C" fn _start(recv_handle: u64) -> ! {
    main(Handle::from_u64(recv_handle));
    unsafe { exit(0) }
}
