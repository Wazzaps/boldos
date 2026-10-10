use alloc::vec::Vec;
use core::arch::naked_asm;
use kernel_api::user::{
    control_thread, create_thread_raw, dump_hex_slice, mm_create, region_create_virtual,
    region_write_full,
};
use kernel_api::{println, ControlThreadOp, CreateThreadFlags, RegionArg, RegionMapFlags};

// #[unsafe(naked)]
// extern "C" fn example_process() -> ! {
//     naked_asm!(
//         "
//         mov x0, #123
//         mov x8, #0
//         svc #0
//         .global example_process_end
//         example_process_end:
//         "
//     );
// }

pub fn process_test(example_bin: Vec<u8>) {
    // let start = example_process as *const () as usize;
    // extern "C" {
    //     static example_process_end: u8;
    // }
    // let end = unsafe { &example_process_end as *const u8 as usize };
    // let size = end - start;
    // let example_process_bytes = unsafe { core::slice::from_raw_parts(start as *const u8, size) };

    // dump_hex_slice(example_process_bytes);

    let code_region_size = example_bin.len().next_multiple_of(4096);
    const BSS_SIZE: usize = 4 * 4096; // TODO: Hack until we have an ELF loader
    println!("Process test: Code region size: {}", code_region_size);
    let code_region =
        region_create_virtual(0, code_region_size + BSS_SIZE).expect("Failed to create region");
    region_write_full(&code_region, &example_bin, 0).expect("Failed to write code region");
    let mut regions = [RegionArg {
        region: code_region.as_u64(),
        size: code_region_size + BSS_SIZE,
        offset: 0,
        addr: 0,
        flags: RegionMapFlags::ReadWrite.bits(),
        #[cfg(target_pointer_width = "64")]
        _padding: 0,
    }];
    let mm = mm_create(&mut regions).expect("Failed to create MM");
    let code_region_addr = regions[0].addr;
    println!("Process test: Code region address: {:#x}", code_region_addr);

    let pid = create_thread_raw(
        code_region_addr,
        0,
        CreateThreadFlags::empty(),
        None,
        Some(&mm),
    )
    .expect("Failed to create thread");
    println!("Process test: Thread created: {}", pid);
    control_thread(pid, ControlThreadOp::Resume).expect("Failed to resume thread");
}
