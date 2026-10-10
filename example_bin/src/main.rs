#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec;
use kernel_api::println;
use kernel_api::user::exit;

fn main() {
    println!(
        "Hello from example_bin! Here's a heap vector: {:?}",
        vec![1, 2, 3]
    );
}

#[no_mangle]
#[link_section = ".text.init"]
pub extern "C" fn _start() -> ! {
    main();
    unsafe { exit(0) }
}
