use alloc::vec::Vec;
use core::arch::naked_asm;
use kernel_api::user::{
    control_thread, create_thread_raw, dump_hex_slice, mm_create, mm_map, port_create, port_send,
    region_create_virtual, region_write_full,
};
use kernel_api::{println, ControlThreadOp, CreateThreadFlags, RegionArg, RegionMapFlags};

use crate::elf_loader::Elf;
use crate::utils::ReadAt;

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

pub fn process_test(example_bin: &dyn ReadAt) {
    let elf = Elf::load(example_bin).expect("Failed to load ELF");

    let (recv_handle, send_handle) = port_create().expect("Failed to create port");
    // println!(
    //     "Process test: Port created: recv={:?}, send={:?}",
    //     recv_handle, send_handle
    // );

    let shmem_handle =
        region_create_virtual(0, 4096).expect("Failed to create shared memory region");
    let shmem = mm_map(None, &shmem_handle, 4096).expect("Failed to map shared memory region");
    println!("Process test: Shared memory mapped to {shmem:?}");
    let shmem = unsafe { core::slice::from_raw_parts_mut(shmem as *mut u8, 4096) };
    shmem[0] = 123;

    let pid = create_thread_raw(
        elf.entry_point,
        0,
        CreateThreadFlags::empty(),
        Some(&[recv_handle]),
        Some(&elf.mm),
    )
    .expect("Failed to create thread");
    println!("Process test: Thread created: {}", pid);
    port_send(
        &send_handle,
        0,
        b"Hello from parent!",
        &[shmem_handle.as_u64()],
    )
    .expect("Failed to send message");
    control_thread(pid, ControlThreadOp::Resume).expect("Failed to resume thread");
}
