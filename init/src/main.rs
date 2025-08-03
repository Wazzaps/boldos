#![no_std]
#![no_main]

mod utils;

use crate::utils::{dump_hex_slice, exit, phy_map, virt_unmap, FmtWriteAdapter};
use core::fmt::Write;
use core::panic::PanicInfo;
use core::ptr::slice_from_raw_parts;
use kernel_api::PhyMapFlags;

fn map_dtb() -> &'static [u8] {
    unsafe {
        // Map just the first page of the DTB, so we can get its size
        const DTB_ADDR: usize = 0x40000000;
        const MAP_LEN: usize = 0x1000;
        let dtb = phy_map(DTB_ADDR, MAP_LEN, PhyMapFlags::empty()).unwrap();
        let dtb_len = {
            let dtb_u32 = dtb as *const u32;
            assert_eq!(*dtb_u32, 0xedfe0dd0 /*0xd00dfeed reversed*/);
            let dtb_len = *dtb_u32.offset(1);
            if dtb_len as usize == MAP_LEN {
                return &*slice_from_raw_parts(dtb as *const u8, MAP_LEN);
            }
            dtb_len
        };
        virt_unmap(dtb, MAP_LEN).unwrap();

        // Map the whole DTB
        let dtb = phy_map(DTB_ADDR, dtb_len as usize, PhyMapFlags::empty()).unwrap();
        &*slice_from_raw_parts(dtb as *const u8, dtb_len as usize)
    }
}

fn main() {
    println!("Hello from usermode!");

    let dtb = map_dtb();
    println!(
        "DTB mapped at {:?}, hexdump of first 32 bytes:",
        dtb.as_ptr()
    );
    dump_hex_slice(&dtb[..32]);
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
