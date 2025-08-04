#![no_std]
#![no_main]

mod utils;

use crate::utils::{
    download_more_ram, dump_hex_slice, exit, phy_map, virt_map, virt_unmap, FmtWriteAdapter,
};
use core::fmt::Write;
use core::panic::PanicInfo;
use core::ptr::slice_from_raw_parts;
use fdt_rs::base::DevTree;
use fdt_rs::error::DevTreeError;
use fdt_rs::prelude::{FallibleIterator, PropReader};
use kernel_api::{PhyMapFlags, VirtMapFlags};

fn map_dtb() -> Result<DevTree<'static>, DevTreeError> {
    unsafe {
        // Map just the first page of the DTB, so we can get its size
        const DTB_ADDR: usize = 0x40000000;
        const MAP_LEN: usize = 0x1000;
        let dtb = phy_map(DTB_ADDR, MAP_LEN, PhyMapFlags::empty()).unwrap() as *const u8;
        let dtb_len = {
            let dtb_len =
                DevTree::read_totalsize(&*slice_from_raw_parts(dtb, DevTree::MIN_HEADER_SIZE))?;
            if dtb_len <= MAP_LEN {
                return Ok(DevTree::new(&*slice_from_raw_parts(dtb, dtb_len))?);
            }
            dtb_len
        };
        virt_unmap(dtb as _, MAP_LEN).unwrap();

        // Map the whole DTB
        let dtb = phy_map(DTB_ADDR, dtb_len, PhyMapFlags::empty()).unwrap() as *const u8;
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

    // Allocate 10 MB
    println!("Allocating big buffer using newly discovered memory");
    let buf = unsafe { virt_map(1024 * 1024 * 10, VirtMapFlags::ReadWrite) }.unwrap();
    println!("10MB Buffer at {buf:?}");
    println!("bye for now...");
}

#[no_mangle]
#[link_section = ".text.init"]
pub extern "C" fn _start() -> ! {
    main();
    unsafe { exit(0) }
}

#[panic_handler]
fn rust_panic(info: &PanicInfo) -> ! {
    let _ = write!(FmtWriteAdapter, "Panic: {}\n", info.message());
    loop {}
}
