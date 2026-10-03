use crate::{
    aarch64::mmu::PageTable,
    page_alloc::{PageBox, PageSlice, PhyAddr},
};
use alloc::{sync::Arc, vec::Vec};
use core::cell::RefCell;
use kernel_api::Pid;
use zerocopy::FromZeros;

pub const PORT_MAX_MESSAGE_SIZE: usize = 64 * 1024;

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
}

pub struct MmRegion {
    pub region: Arc<RefCell<Region>>,
    pub addr: usize,
    pub size: usize,
}

#[derive(FromZeros, Clone)]
pub struct MmHandle {}

#[derive(FromZeros, Clone)]
pub struct WaiterHandle {}
