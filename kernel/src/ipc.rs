use crate::{
    aarch64::mmu::PageTable,
    page_alloc::{PageBox, PageSlice, PhyAddr},
};
use alloc::{sync::Arc, vec::Vec};
use core::cell::RefCell;
use kernel_api::Pid;
use zerocopy::FromZeros;

pub const PORT_MAX_MESSAGE_SIZE: usize = 64 * 1024;
pub const MAX_HANDLES: usize = 10 * 1024;

pub struct Futex {
    pub phy_addr: PhyAddr,
    pub pid: Pid,
    pub value: u32,
}

pub struct Handle {
    pub id: u64,
    pub flags: u16,
    pub data: HandleData,
}

impl Handle {
    pub fn duplicate(&self) -> Handle {
        // TODO: Deny duplicating certain ports
        Handle {
            id: self.id,
            flags: self.flags,
            data: self.data.clone(),
        }
    }
}

#[derive(Clone)]
pub enum HandleData {
    Port(PortHandle),
    Region(RegionHandle),
    Mm(MmHandle),
    Waiter(WaiterHandle),
}

#[derive(FromZeros)]
pub struct Port {
    pub recv_pid: Pid,
    pub recv_handle: u64,
    pub buffer_len: usize,
    pub buffer: PageSlice,
}

#[derive(Clone)]
pub struct PortHandle {
    pub port: Arc<RefCell<Port>>,
}

impl PortHandle {
    pub const FLAG_RECV: u16 = 0;
    pub const FLAG_SEND: u16 = 1;
    pub const FLAG_SEND_ONCE: u16 = 2;
}

pub struct Region {
    pub data: PageSlice,
    pub owned: bool,
}

#[derive(Clone)]
pub struct RegionHandle {
    pub region: Arc<RefCell<Region>>,
}

pub struct Mm {
    pub page_table: PageBox<PageTable>,
    pub regions: Vec<MmRegion>,
}

impl Mm {
    pub fn new() -> Self {
        let page_table = PageBox::<PageTable>::new_zeroed();
        page_table.as_ref().init_meta(0);
        Self {
            page_table,
            regions: Vec::new(),
        }
    }

    unsafe fn add_region(&mut self, mm_region: MmRegion) {
        self.regions.push(mm_region);
    }

    unsafe fn remove_region(&mut self, remove_start: usize, remove_end: usize) {
        let mut i = 0;
        while i < self.regions.len() {
            let region = &mut self.regions[i];
            let region_start = region.addr;
            let region_end = region.addr + region.size;

            if remove_end <= region_start || remove_start >= region_end {
                // No overlap, move to the next region
                i += 1;
            } else if remove_start <= region_start && remove_end >= region_end {
                // Whole region is covered by the removed range, remove it
                self.regions.swap_remove(i);
            } else if remove_start <= region_start {
                // Start of the region is covered by the removed range, shrink the region
                region.addr = remove_end;
                region.size = region_end - remove_end;
                i += 1;
            } else if remove_end >= region_end {
                // End of the region is covered by the removed range, shrink the region
                region.size = remove_start - region.addr;
                i += 1;
            } else {
                // Middle of the region is covered by the removed range, split the region
                let new_region = MmRegion {
                    addr: remove_end,
                    size: region_end - remove_end,
                    region: Arc::clone(&region.region),
                };
                region.size = remove_start - region.addr;
                self.regions.push(new_region);
                i += 1;
            }
        }
    }

    unsafe fn vmap(&mut self, phy_addr: PhyAddr, len: usize, flags: u64) -> usize {
        self.page_table.as_mut().vmap(phy_addr, len, flags)
    }

    unsafe fn vunmap(&mut self, virt_addr: usize, len: usize) {
        self.page_table.as_mut().vunmap(virt_addr, len);
    }

    pub unsafe fn map(
        &mut self,
        phy_addr: PhyAddr,
        size: usize,
        page_flags: u64,
        region: Arc<RefCell<Region>>,
    ) -> usize {
        let virt_addr = self.vmap(phy_addr, size, page_flags);
        // Ensure the region doesn't get freed
        self.add_region(MmRegion {
            region,
            addr: virt_addr,
            size,
        });
        virt_addr
    }

    pub unsafe fn unmap(&mut self, virt_addr: usize, len: usize) {
        self.vunmap(virt_addr, len);
        self.remove_region(virt_addr, virt_addr.saturating_add(len));
    }
}

pub struct MmRegion {
    pub region: Arc<RefCell<Region>>,
    pub addr: usize,
    pub size: usize,
}

#[derive(Clone)]
pub struct MmHandle {
    pub mm: Arc<RefCell<Mm>>,
}

#[derive(Clone)]
pub struct WaiterHandle {}
