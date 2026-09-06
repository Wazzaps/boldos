use crate::println;
use crate::utils::mem_map;
use buddy_system_allocator::LockedHeap;
use core::alloc::{GlobalAlloc, Layout};
use kernel_api::MemMapFlags;

static HEAP_ALLOCATOR: LockedHeap<33> = LockedHeap::new();

#[global_allocator]
static MEMMAPPING_ALLOCATOR: MemMappingAllocator = MemMappingAllocator;

struct MemMappingAllocator;

unsafe impl GlobalAlloc for MemMappingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut allocator = HEAP_ALLOCATOR.lock();
        match allocator.alloc(layout) {
            Ok(ptr) => ptr.as_ptr(),
            Err(_) => {
                // Expand and try again. The buddy alloc needs a block at least twice as big as the allocation.
                let new_size = (layout.size().next_power_of_two() + 1)
                    .next_power_of_two()
                    .max(1024 * 1024);
                println!("Expanding heap by {} bytes", new_size);
                let buffer = mem_map(new_size, MemMapFlags::ReadWrite)
                    .expect("Failed to map memory for heap");
                println!("Mapped memory at {:p}", buffer);
                allocator.init(buffer as usize, new_size);

                allocator
                    .alloc(layout)
                    .expect("Failed to allocate memory")
                    .as_ptr()
            }
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        HEAP_ALLOCATOR.dealloc(ptr, layout);
    }
}
