#![feature(naked_functions)]
#![no_std]
#![no_main]

use crate::drv::virtio::Virtio9pDriver;
use crate::page_alloc::{alloc, BitmapPageAlloc, PAGE_ALLOC};
use aarch64_cpu::registers::{CurrentEL, MAIR_EL1, TTBR0_EL1};
use core::arch::{asm, naked_asm};
use core::fmt::Write;
use core::panic::PanicInfo;
use drv::virtio::VirtioMmioDev;
use fdt_rs::base::DevTree;
use fdt_rs::prelude::{FallibleIterator, PropReader};
use tock_registers::interfaces::{Readable, Writeable};

mod drv;
mod page_alloc;
mod prelude;

#[allow(dead_code)]
#[used]
#[no_mangle]
#[link_section = ".bss"]
pub static mut MOCK: u32 = 0;

#[no_mangle]
#[naked]
#[link_section = ".text.init"]
pub extern "C" fn _start() -> ! {
    unsafe {
        naked_asm!(
            "
            // Erase bss
            ldr x10, =_bss_start
            ldr x11, =_bss_end
            cmp x10, x11
            beq 2f
            1:
            str xzr, [x10], #8
            cmp x10, x11
            bne 1b
            2:

            // Set stack and call main
            ldr x10, =_initstack_end
            mov sp, x10
            b main
            ",
        )
    }
}

extern "C" {
    static _dtb_start: u8;
    static _text_start: u8;
    static _end: u8;
}

#[no_mangle]
pub extern "C" fn main() -> ! {
    println!("--- Booting ---");

    let curr_el = CurrentEL.read(CurrentEL::EL);
    assert_eq!(curr_el, 1, "Unexpectedly booted in EL{}", curr_el);

    // Read DTB
    let dtb = unsafe {
        let dtb_base: *const u8 = &_dtb_start as *const u8;
        let dtb_header = core::slice::from_raw_parts(dtb_base, DevTree::MIN_HEADER_SIZE);
        let dtb_size = DevTree::read_totalsize(dtb_header).unwrap();
        println!("Loading DevTree of size 0x{:x}", dtb_size);
        DevTree::new(core::slice::from_raw_parts(dtb_base, dtb_size)).unwrap()
    };

    // Find bootargs & memory node
    let mut bootargs = None;
    let mut mem = None;
    let mut node_iter = dtb.nodes();
    while let Some(node) = node_iter.next().unwrap() {
        let node_name = node.name().unwrap();
        // println!("---- {}", node_name);
        if node_name == "chosen" {
            let mut prop_iter = node.props();
            while let Some(prop) = prop_iter.next().unwrap() {
                if prop.name().unwrap() == "bootargs" {
                    bootargs = prop.iter_str().next().unwrap();
                    break;
                }
                // println!("  {}: {:?}", prop.name().unwrap(), prop.iter_str());
            }
        } else if node_name.starts_with("memory@") {
            let mut prop_iter = node.props();
            while let Some(prop) = prop_iter.next().unwrap() {
                if prop.name().unwrap() == "reg" {
                    mem = Some((prop.u64(0).unwrap(), prop.u64(1).unwrap()));
                    break;
                }
            }
        }
        // let mut prop_iter = node.props();
        // while let Some(prop) = prop_iter.next().unwrap() {
        //     println!("  {}: {:?}", prop.name().unwrap(), prop.iter_str());
        // }
    }

    // Print bootargs
    if let Some(bootargs) = bootargs {
        println!("Boot args: {}", bootargs);
    } else {
        println!("No boot args");
    }

    // Init page allocator
    let mem = mem.expect("memory node not found");
    println!("RAM: 0x{:x} (0x{:x} bytes)", mem.0, mem.1);
    assert_eq!(mem.1, 0x10000000, "FIXME: Must have exactly 256MiB of ram");
    #[allow(static_mut_refs)]
    unsafe {
        PAGE_ALLOC.rebase(mem.0 as usize)
    };

    // Mark kernel as allocated
    unsafe {
        let text_start = &_text_start as *const u8 as usize;
        debug_assert!(text_start % 0x1000 == 0, "text_start must be page-aligned");
        let text_end = &_end as *const u8 as usize;
        let text_end = (text_end + 0xfff) & !0xfff;
        #[allow(static_mut_refs)]
        PAGE_ALLOC.mark_allocated(text_start, (text_end - text_start) / 0x1000);
    }

    // Mark DTB as allocated
    unsafe {
        let dtb_start = &_dtb_start as *const u8 as usize;
        debug_assert!(dtb_start % 0x1000 == 0, "dtb_start must be page-aligned");
        let dtb_end = dtb_start + dtb.totalsize();
        let dtb_end = (dtb_end + 0xfff) & !0xfff;
        #[allow(static_mut_refs)]
        PAGE_ALLOC.mark_allocated(dtb_start, (dtb_end - dtb_start) / 0x1000);
    }

    // Init MMU
    // #0   - Normal memory (write-back cachable normal-memory non-transient)
    // #1-8 - Device memory (nGnRnE)
    MAIR_EL1.set(0b00000000_00000000_00000000_00000000_00000000_00000000_00000000_11111111);
    let mut root_page = alloc(1);

    // println!("{:p}", root_page.as_ptr());

    {
        let block_size: u64 = 0x40000000000;

        const PF_TYPE_BLOCK: u64 = 1 << 0;
        const MT_NORMAL: u64 = 0;
        const PF_MEM_TYPE_NORMAL: u64 = MT_NORMAL << 2;
        const PF_READ_WRITE: u64 = 1 << 6;
        const PF_INNER_SHAREABLE: u64 = 3 << 8;
        const PF_ACCESS_FLAG: u64 = 1 << 10;

        let block_attr = PF_TYPE_BLOCK
            | PF_MEM_TYPE_NORMAL
            | PF_READ_WRITE
            | PF_INNER_SHAREABLE
            | PF_ACCESS_FLAG;

        unsafe {
            for i in 0..512 {
                let page = root_page.as_mut_ptr() as *mut u64;
                // println!("writing to {:p}", page.offset(i));
                page.offset(i).write((block_size * i as u64) | block_attr);
            }
        }
    }

    TTBR0_EL1.set(root_page.as_ptr() as u64);

    // Make sure ram is unused
    #[cfg(debug_assertions)]
    unsafe {
        #[allow(static_mut_refs)]
        PAGE_ALLOC.overwrite_free_pages();
    }

    // TODO: init smp

    // // Find 9p virtio mmio base
    // let mut virtio_9p = None;
    // let mut node_iter = dtb.compatible_nodes("virtio,mmio");
    // while let Some(node) = node_iter.next().unwrap() {
    //     let mut prop_iter = node.props();
    //     while let Some(prop) = prop_iter.next().unwrap() {
    //         if prop.name().unwrap() == "reg" {
    //             let (dev, info) = unsafe { VirtioMmioDev::new(prop.u64(0).unwrap() as *mut ()) };
    //             if info.device_id == 9 {
    //                 virtio_9p = Some(Virtio9pDriver::new(dev));
    //                 break;
    //             }
    //         }
    //     }
    // }
    // let virtio_9p = virtio_9p.expect("virtio_9p not found");
    // println!("Virtio 9p mmio: {:?}", virtio_9p);

    loop {
        unsafe { asm!("wfi") }
    }
}

#[panic_handler]
fn rust_panic(info: &PanicInfo) -> ! {
    print!("[PANIC]: ");
    let _ = write!(drv::qemu_console::FmtWriteAdapter, "{}", info.message());
    print!("\n");

    loop {
        unsafe { asm!("wfi") }
    }
}
