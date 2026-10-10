use crate::{
    println,
    utils::{
        control_thread, create_thread, mm_map, mm_unmap_range, port_create, port_recv, port_send,
        region_create_physical, region_create_virtual, region_get_size, region_read_full,
        region_write_full, sleep, AsciiStr,
    },
};
use alloc::format;
use core::time::Duration;
use kernel_api::{ControlThreadOp, KError};

pub fn ipc_test() -> ! {
    println!("ipc_test: Starting");

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
    mm_unmap_range(None, mapped_ptr, 8192).expect("Failed to unmap mapped region");
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

    // Region map stress test
    for i in 0..100 {
        let region = region_create_virtual(0, 8192).expect("Failed to create region");
        // println!("ipc_test: Region created: {:?}", region);
        let mapped_ptr = mm_map(None, &region, 8192).expect("Failed to map region");
        if i % 10 == 0 {
            println!("ipc_test[{i}]: mapped region to {mapped_ptr:?}");
        }
        mm_unmap_range(None, mapped_ptr, 8192).expect("Failed to unmap mapped region");
        drop(region);
    }

    let (rx, tx) = port_create().expect("Failed to create port");
    println!("ipc_test: Port created: {:?}, {:?}", rx, tx);

    let pid = create_thread(move || {
        sleep(Duration::from_millis(1100));
        let mut i = 0u64;
        loop {
            match port_send(&tx, 0, format!("Hello, world! {i}").as_bytes(), &[]) {
                Ok(_) => println!("ipc_test: Send went OK"),
                Err(KError::PortFull) => println!("ipc_test: Port full"),
                Err(e) => panic!("ipc_test: Unexpected error: {:?}", e),
            }
            i += 1;
            sleep(Duration::from_millis(1234));
        }
    })
    .expect("Failed to create thread");
    println!("ipc_test: Thread created: {}", pid);
    control_thread(pid, ControlThreadOp::Resume).expect("Failed to resume thread");

    let mut buf = [0u8; 128];
    loop {
        match port_recv(&rx, 0, &mut buf, &mut []) {
            Ok((num_bytes, num_handles)) => {
                println!(
                    "ipc_test: Received {num_bytes} bytes: '{}' + {num_handles} handles",
                    AsciiStr(&buf[..num_bytes]),
                );
            }
            Err(KError::PortEmpty) => println!("ipc_test: Port empty"),
            Err(e) => panic!("ipc_test: Unexpected error: {:?}", e),
        }
        sleep(Duration::from_millis(500));
    }
}
