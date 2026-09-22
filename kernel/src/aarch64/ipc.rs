use crate::page_alloc::PhyAddr;
use core::mem::ManuallyDrop;
use kernel_api::Pid;
use zerocopy::FromZeros;

#[derive(FromZeros)]
pub struct Futex {
    pub phys_addr: PhyAddr,
    pub pid: Pid,
    pub value: u32,
}

#[derive(FromZeros)]
pub struct Handle {
    pub id: u64,
    pub flags: u16,
    pub handle_type: u8,
    data: RawHandleData,
}

impl Handle {
    const HANDLE_TYPE_PORT: u8 = 1;
    const HANDLE_TYPE_REGION: u8 = 2;
    const HANDLE_TYPE_MM: u8 = 3;
    const HANDLE_TYPE_WAITER: u8 = 4;

    pub fn data(&self) -> HandleDataRef<'_> {
        unsafe {
            match self.handle_type {
                Self::HANDLE_TYPE_PORT => HandleDataRef::Port(&self.data.port),
                Self::HANDLE_TYPE_REGION => HandleDataRef::Region(&self.data.region),
                Self::HANDLE_TYPE_MM => HandleDataRef::Mm(&self.data.mm),
                Self::HANDLE_TYPE_WAITER => HandleDataRef::Waiter(&self.data.waiter),
                _ => panic!("Invalid handle type: {}", self.handle_type),
            }
        }
    }

    pub fn set_data(&mut self, data: HandleDataRef<'_>) {
        match data {
            HandleDataRef::Port(port) => {
                self.data.port = ManuallyDrop::new(port.clone());
                self.handle_type = Self::HANDLE_TYPE_PORT;
            }
            HandleDataRef::Region(region) => {
                self.data.region = ManuallyDrop::new(region.clone());
                self.handle_type = Self::HANDLE_TYPE_REGION;
            }
            HandleDataRef::Mm(mm) => {
                self.data.mm = ManuallyDrop::new(mm.clone());
                self.handle_type = Self::HANDLE_TYPE_MM;
            }
            HandleDataRef::Waiter(waiter) => {
                self.data.waiter = ManuallyDrop::new(waiter.clone());
                self.handle_type = Self::HANDLE_TYPE_WAITER;
            }
        }
    }
}

#[derive(FromZeros)]
pub union RawHandleData {
    port: ManuallyDrop<PortHandle>,
    region: ManuallyDrop<RegionHandle>,
    mm: ManuallyDrop<MmHandle>,
    waiter: ManuallyDrop<WaiterHandle>,
}

pub enum HandleDataRef<'a> {
    Port(&'a PortHandle),
    Region(&'a RegionHandle),
    Mm(&'a MmHandle),
    Waiter(&'a WaiterHandle),
}

#[derive(FromZeros)]
pub struct Port {
    pub recv_pid: Pid,
    pub recv_handle: u64,
    pub ref_count: u32,
}

#[derive(FromZeros, Clone)]
pub struct PortHandle {
    pub port: *mut Port,
}

impl PortHandle {
    pub const FLAG_RECV: u16 = 0;
    pub const FLAG_SEND: u16 = 1;
    pub const FLAG_SEND_ONCE: u16 = 2;
}

#[derive(FromZeros, Clone)]
pub struct RegionHandle {}

#[derive(FromZeros, Clone)]
pub struct MmHandle {}

#[derive(FromZeros, Clone)]
pub struct WaiterHandle {}
