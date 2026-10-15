#![no_std]
#![no_main]

extern crate alloc;

mod drv;
mod elf_loader;
mod ipc_test;
mod process_test;
mod vfs;

use crate::drv::gic::GicAndTimer;
use crate::drv::qemu_fwcfg::QemuFwCfg;
use crate::drv::virtio::VirtioNinep;
use alloc::vec::Vec;
use core::ptr::slice_from_raw_parts;
use core::sync::atomic::{AtomicU32, Ordering};
use core::time::Duration;
use fdt_rs::base::DevTree;
use fdt_rs::error::DevTreeError;
use fdt_rs::prelude::{FallibleIterator, PropReader};
use kernel_api::user::{
    control_thread, create_thread, download_more_ram, dump_hex_slice, exit, futex_wait, futex_wake,
    get_pid, mem_unmap, phy_map, sleep,
};
use kernel_api::{println, ControlThreadOp, PhyMapFlags};

static mut GLOBAL_COUNTER: u64 = 0;
static FUTEX_WORD: AtomicU32 = AtomicU32::new(0);

fn map_dtb() -> Result<DevTree<'static>, DevTreeError> {
    unsafe {
        // Map just the first page of the DTB, so we can get its size
        const DTB_ADDR: usize = 0x40000000;
        const MAP_LEN: usize = 0x1000;
        let dtb = phy_map(DTB_ADDR, MAP_LEN, PhyMapFlags::empty()).unwrap().0 as *const u8;
        let dtb_len = {
            let dtb_len =
                DevTree::read_totalsize(&*slice_from_raw_parts(dtb, DevTree::MIN_HEADER_SIZE))?;
            if dtb_len <= MAP_LEN {
                return Ok(DevTree::new(&*slice_from_raw_parts(dtb, dtb_len))?);
            }
            dtb_len
        };
        mem_unmap(dtb as _, MAP_LEN).unwrap();

        // Map the whole DTB
        let dtb = phy_map(DTB_ADDR, dtb_len, PhyMapFlags::empty()).unwrap().0 as *const u8;
        Ok(DevTree::new(&*slice_from_raw_parts(dtb, dtb_len))?)
    }
}

fn find_mem_nodes(dtb: &DevTree) -> Result<(), DevTreeError> {
    let mut node_iter = dtb.nodes();
    let mut bootargs = None;
    let mut mem = None;
    while let Some(node) = node_iter.next()? {
        let node_name = node.name()?;
        if node_name == "chosen" {
            let mut prop_iter = node.props();
            while let Some(prop) = prop_iter.next()? {
                if prop.name()? == "bootargs" {
                    bootargs = prop.iter_str().next()?;
                    break;
                }
                // println!("  {}: {:?}", prop.name()?, prop.iter_str());
            }
        } else if node_name.starts_with("memory@") {
            let mut prop_iter = node.props();
            while let Some(prop) = prop_iter.next()? {
                if prop.name()? == "reg" {
                    mem = Some((prop.u64(0)?, prop.u64(1)?));
                    break;
                }
            }
        }
    }

    if let Some(bootargs) = bootargs {
        println!("Boot args: {:?}", bootargs);
    } else {
        println!("No boot args");
    }
    let mem = mem.expect("device tree did not contain memory node");
    println!("RAM: 0p{:x} ({} bytes)", mem.0, mem.1);

    // Tell the kernel about the memory node
    unsafe { download_more_ram(mem.0 as usize, mem.1 as usize) }.unwrap();

    Ok(())
}

fn main() {
    println!("Hello from usermode!");

    let dtb = map_dtb().expect("Failed to parse device tree");
    println!(
        "DTB mapped at {:?}, hexdump of first 32 bytes:",
        dtb.buf().as_ptr()
    );
    dump_hex_slice(&dtb.buf()[..32]);

    // Find all memory nodes
    find_mem_nodes(&dtb).expect("Failed to parse device tree");

    // Try the heap
    let v = alloc::vec![1, 2, 3];
    println!("Heap vector: {:?}", v);

    // Find all devices
    let _gic_and_timer = GicAndTimer::find_and_init(&dtb).expect("Failed to parse device tree");
    // let mut _qemu_fwcfg = QemuFwCfg::find_and_init(&dtb).expect("Failed to parse device tree");
    // _qemu_fwcfg.dump_files();
    let mut ninep = VirtioNinep::find(&dtb).expect("Failed to find Virtio 9p");
    let root = ninep.init();
    // let root_dir = ninep.open_file(root, b"", VirtioNinep::O_RDONLY | VirtioNinep::O_DIRECTORY);
    // for item in ninep.read_dir(root_dir, 0) {
    //     println!("Item: {:?}", item);
    // }

    let file = ninep.open_file(root, b"example-bin.elf", VirtioNinep::O_RDONLY);
    let mut binary = Vec::new();
    loop {
        let chunk = ninep.read_file(file, binary.len() as u64, 64 * 1024);
        if chunk.is_empty() {
            break;
        }
        binary.extend_from_slice(&chunk);
    }

    // println!("Creating thread");
    // let tid = create_thread(|| {
    //     println!("Hello from thread! My PID is {}", get_pid());
    //     for _ in 0..6 {
    //         let counter = unsafe { *(&raw const GLOBAL_COUNTER) };
    //         println!(
    //             "Thread Current time: {} ms, counter: {}",
    //             GicAndTimer::current_time_ms(),
    //             counter
    //         );
    //         // delay_ticks(500000000);
    //         sleep(Duration::from_millis(100));
    //     }
    //     unsafe { exit(0) };
    // })
    // .expect("Failed to create thread");
    // control_thread(tid, ControlThreadOp::Resume).expect("Failed to resume thread");
    // println!("Thread created with ID: {}", tid);

    // println!("Creating futex waiter thread");
    // let futex_waiter_tid = create_thread(|| {
    //     println!("Hello from futex waiter thread! My PID is {}", get_pid());
    //     loop {
    //         let is_ready = futex_wait(
    //             FUTEX_WORD.as_ptr(),
    //             0,
    //             Duration::from_millis(150).as_micros() as u64,
    //         )
    //         .is_ok();
    //         println!(
    //             "Futex waiter thread woke up, is_ready: {}, value: {}",
    //             is_ready,
    //             FUTEX_WORD.load(Ordering::Relaxed)
    //         );
    //         if is_ready {
    //             break;
    //         }
    //     }

    //     unsafe { exit(0) };
    // })
    // .expect("Failed to create thread");
    // control_thread(futex_waiter_tid, ControlThreadOp::Resume).expect("Failed to resume thread");
    // println!("Futex waiter thread created with ID: {}", futex_waiter_tid);

    // for _ in 0..5 {
    //     let counter = unsafe {
    //         let counter_ptr = &raw mut GLOBAL_COUNTER;
    //         *counter_ptr += 1;
    //         *counter_ptr
    //     };
    //     println!(
    //         "Current time: {} ms, counter: {}",
    //         GicAndTimer::current_time_ms(),
    //         counter
    //     );

    //     // Wake up the futex waiter thread
    //     if counter == 3 {
    //         println!("Waking up the futex waiter thread");
    //         FUTEX_WORD.store(123, Ordering::Relaxed);
    //         futex_wake(FUTEX_WORD.as_ptr(), u32::MAX).unwrap();
    //     }

    //     // delay_ticks(500000000);
    //     sleep(Duration::from_millis(100));
    // }

    // ipc_test::ipc_test();
    // let vfs_port = vfs::spawn_vfs_server().expect("Failed to spawn VFS server");
    // vfs::vfs_test(vfs_port);
    process_test::process_test(binary);
    loop {
        sleep(Duration::from_secs(9999));
    }
}

#[no_mangle]
#[link_section = ".text.init"]
pub extern "C" fn _start() -> ! {
    main();
    exit(0);
}
