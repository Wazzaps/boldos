use crate::page_alloc::{alloc_zeroed, PAGE_SIZE};
use crate::println;
use buddy_system_allocator::LockedHeap;
use core::alloc::{GlobalAlloc, Layout};

static HEAP_ALLOCATOR: LockedHeap<33> = LockedHeap::new();

#[global_allocator]
static KERNEL_ALLOCATOR: KernelAllocator = KernelAllocator;

struct KernelAllocator;

unsafe impl GlobalAlloc for KernelAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut allocator = HEAP_ALLOCATOR.lock();
        match allocator.alloc(layout) {
            Ok(ptr) => ptr.as_ptr(),
            Err(_) => {
                // Expand and try again. The buddy alloc needs a block at least twice as big as the allocation.
                let new_size = (layout.size().next_power_of_two() + 1)
                    .next_power_of_two()
                    .max(4 * PAGE_SIZE);
                println!("alloc: Expanding kernel heap by {} bytes", new_size);
                let buffer = alloc_zeroed(new_size / PAGE_SIZE);
                // println!("alloc: Mapped memory at {:p}", buffer);
                allocator.init(buffer.as_ptr() as usize, new_size);
                core::mem::forget(buffer);

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
