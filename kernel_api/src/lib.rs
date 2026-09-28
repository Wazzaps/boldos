#![no_std]
use bitflags::bitflags;
use core::fmt::Debug;
use num_enum::{FromPrimitive, IntoPrimitive, TryFromPrimitive};
use zerocopy::{FromBytes, Immutable, IntoBytes};

pub type Pid = u32;
pub type Handle = u64;

// TODO: rename to NounVerb
#[derive(TryFromPrimitive, IntoPrimitive, Eq, PartialEq, Copy, Clone, Debug)]
#[repr(u32)]
pub enum Syscall {
    Exit = 0,
    Log = 1,
    PhyMap = 2,   // Replaced by region_create_physical
    MemMap = 3,   // Replaced by region_create_virtual + mm_modify
    MemUnmap = 4, // Replaced by mm_modify
    DownloadMoreRam = 5,
    LoadKernelDevice = 6,
    SleepMs = 7, // Replaced by anonymous waiter_wait
    CreateThread = 8,
    GetPid = 9,
    ControlThread = 10,
    VirtToPhys = 11,
    Futex = 12, // Replaced by anonymous waiter_wait

    HandleDuplicate = 13,
    HandleClose = 14,
    PortCreate = 15,
    PortRecv = 16,
    PortSend = 17,
    RegionCreateVirtual = 18,
    RegionCreatePhysical = 19,
    RegionGetSize = 20,
    RegionRead = 21,
    RegionWrite = 22,
    MmCreate = 23,
    MmModify = 24,
    WaiterCreate = 25,
    WaiterWait = 26,
}

#[derive(FromPrimitive, IntoPrimitive, Eq, PartialEq, Copy, Clone, Debug)]
#[repr(i32)]
pub enum KError {
    #[num_enum(catch_all)]
    Unknown(i32),
    AlreadyExists = -1,
    OOM = -2,
    InvalidArgument = -3,
    InvalidAddress = -4,
    TryAgain = -5,
    PortFull = -6,
    PortEmpty = -7,
    MissingPermission = -8,
    InvalidHandle = -9,
    TooSmall = -10,
}

impl Into<u64> for KError {
    fn into(self) -> u64 {
        Into::<i32>::into(self) as u64
    }
}

#[derive(Debug, FromBytes, IntoBytes, Immutable)]
#[repr(C)]
pub struct RegionArg {
    pub region: Handle,
    pub size: usize,
    pub offset: usize,
    // Specify 0 for arbitrary address
    pub addr: usize,
    pub flags: u32,
    #[cfg(target_pointer_width = "64")]
    pub _padding: u32,
}

#[derive(TryFromPrimitive, IntoPrimitive, Eq, PartialEq, Copy, Clone, Debug)]
#[repr(u32)]
pub enum ControlThreadOp {
    Pause = 0,
    Resume = 1,
}

pub mod kernel_device {
    use zerocopy::{FromBytes, IntoBytes};

    pub trait KernelDeviceId {
        const ID: u32;
    }

    // const GIC_AND_TIMER_PPI_INTERRUPT: u32 = 1 << 0;

    #[derive(Debug, FromBytes, IntoBytes)]
    #[repr(C)]
    pub struct GicAndTimer {
        pub gicd_base: u64,
        // pub gicd_size: u64,
        pub gicc_base: u64,
        pub timer_ppi_interrupt: u32,
        // pub gicc_size: u64,
        // pub flags: u32,
        pub _padding: u32,
    }

    impl KernelDeviceId for GicAndTimer {
        const ID: u32 = 1;
    }
}

bitflags! {
    pub struct PhyMapFlags: u64 {
        const ReadWrite = 1 << 0;
        const DeviceMem = 1 << 1;
    }
    pub struct MemMapFlags: u64 {
        const ReadWrite = 1 << 0;
    }
    pub struct CreateThreadFlags: u64 {
        const SharePageTable = 1 << 0;
        const ShareHandles = 1 << 1;
    }
    pub struct FutexOp: u64 {
        const WAIT = 0;
        const WAKE = 1;
    }
    pub struct RegionMapFlags: u64 {
        const ReadWrite = 1 << 0;
    }
}
