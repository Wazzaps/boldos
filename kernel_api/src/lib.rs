#![no_std]
use bitflags::bitflags;
use num_enum::{FromPrimitive, IntoPrimitive, TryFromPrimitive};
use zerocopy::{FromBytes, Immutable, IntoBytes};

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

#[derive(TryFromPrimitive, IntoPrimitive, Eq, PartialEq, Copy, Clone, Debug)]
#[repr(u32)]
pub enum KernelDeviceType {
    Invalid = 0,
    Timer = 1,
}

#[derive(Debug, FromBytes, IntoBytes, Immutable)]
#[repr(C)]
pub struct KernelDeviceRequest {
    pub dev_type: u32, // KernelDeviceType
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
