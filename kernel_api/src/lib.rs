#![no_std]
use bitflags::bitflags;
use core::fmt::Debug;
use num_enum::{FromPrimitive, IntoPrimitive, TryFromPrimitive};

#[derive(TryFromPrimitive, IntoPrimitive, Eq, PartialEq, Copy, Clone, Debug)]
#[repr(u32)]
pub enum Syscall {
    Exit = 0,
    Log = 1,
    PhyMap = 2,
    MemMap = 3,
    MemUnmap = 4,
    DownloadMoreRam = 5,
    LoadKernelDevice = 6,
    SleepSec = 7,
}

#[derive(FromPrimitive, IntoPrimitive, Eq, PartialEq, Copy, Clone, Debug)]
#[repr(i32)]
pub enum KError {
    #[num_enum(catch_all)]
    Unknown(i32),
    AlreadyExists = -1,
    OOM = -2,
    InvalidArgument = -3,
}

impl Into<u64> for KError {
    fn into(self) -> u64 {
        Into::<i32>::into(self) as u64
    }
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
}
