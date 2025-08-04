#![no_std]
use bitflags::bitflags;
use num_enum::{FromPrimitive, IntoPrimitive, TryFromPrimitive};

#[derive(TryFromPrimitive, IntoPrimitive, Eq, PartialEq, Copy, Clone, Debug)]
#[repr(u32)]
pub enum Syscall {
    Exit = 0,
    Log = 1,
    PhyMap = 2,
    VirtMap = 3,
    VirtUnmap = 4,
    DownloadMoreRam = 5,
}

#[derive(FromPrimitive, IntoPrimitive, Eq, PartialEq, Copy, Clone, Debug)]
#[repr(i32)]
pub enum KError {
    #[num_enum(catch_all)]
    Unknown(i32),
    AlreadyExists = -1,
    OOM = -2,
}

#[derive(Debug, Eq, PartialEq, FromPrimitive)]
#[repr(u8)]
enum Number {
    Zero = 0,
    #[num_enum(catch_all)]
    NonZero(u8),
}

bitflags! {
    pub struct PhyMapFlags: u64 {
        const ReadWrite = 1 << 0;
        const DeviceMem = 1 << 1;
    }
    pub struct VirtMapFlags: u64 {
        const ReadWrite = 1 << 0;
    }
}
