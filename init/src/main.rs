#![no_std]
#![no_main]

mod utils;

use crate::utils::{dump_hex_slice, phy_map, FmtWriteAdapter};
use core::fmt::Write;
use core::panic::PanicInfo;
use core::ptr::slice_from_raw_parts;
use kernel_api::PhyMapFlags;

#[no_mangle]
#[link_section = ".text.init"]
pub extern "C" fn _start() -> ! {
    unsafe {
        println!("Hello from usermode!");
        const DTB_ADDR: u64 = 0x40000000;
        const MAP_LEN: usize = 0x1000;
        let dtb = phy_map(DTB_ADDR, MAP_LEN as u64, PhyMapFlags::empty()).unwrap() as *const u8;
        println!("DTB mapped at {:?}, hexdump of first 32 bytes:", dtb);
        let dtb = &*slice_from_raw_parts(dtb, MAP_LEN);
        dump_hex_slice(&dtb[..32]);
        println!();
    }
    loop {}
}

#[panic_handler]
fn rust_panic(info: &PanicInfo) -> ! {
    let _ = write!(FmtWriteAdapter, "Panic: {}\n", info.message());
    loop {}
}
