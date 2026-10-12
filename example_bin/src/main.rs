#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec;
use kernel_api::user::{exit, mm_map, port_recv, region_get_size, AsciiStr, Handle};
use kernel_api::{println, HandleId};

fn main(handles: &[HandleId]) {
    let recv_handle = Handle::from_u64(handles[0]);
    println!(
        "Hello from example_bin! Here's a heap vector: {:?}",
        vec![1, 2, 3]
    );

    let mut buf: [u8; 128] = [0u8; 128];
    let mut handles: [HandleId; 1] = [0; 1];
    let (len, handle_count) =
        port_recv(&recv_handle, 0, &mut buf, &mut handles).expect("Failed to receive message");
    println!("Received message from parent: {}", AsciiStr(&buf[..len]));
    assert_eq!(handle_count, 1);
    let shmem_handle = Handle::from_u64(handles[0]);
    // println!("Received shmem handle: {:?}", shmem_handle);
    let shmem_size = region_get_size(&shmem_handle).expect("Failed to get shmem size");
    // println!("Shmem size: {:?}", shmem_size);
    let shmem = mm_map(None, &shmem_handle, shmem_size).expect("Failed to map shmem") as *mut u8;
    unsafe {
        let val = *shmem;
        println!("Shmem value: {:?}", val);
    }
}

#[no_mangle]
#[link_section = ".text.init"]
pub extern "C" fn _start(handles: *const HandleId, handles_len: u64) -> ! {
    let handles = unsafe { core::slice::from_raw_parts(handles, handles_len as usize) };
    // println!("Example bin: handles: {:?}", handles);
    main(handles);
    exit(0);
}
