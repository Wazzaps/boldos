use core::fmt::Debug;

pub struct VirtioMmioDev {
    base: *mut u32,
}

pub struct VirtioMmioDevInfo {
    pub device_id: u32,
}

impl VirtioMmioDev {
    pub unsafe fn new(base: *mut ()) -> (Self, VirtioMmioDevInfo) {
        let virtio = Self {
            base: base as *mut u32,
        };
        assert_eq!(
            unsafe { virtio.read_u32(0) },
            0x74726976,
            "virtio signature mismatch"
        );
        assert_eq!(
            unsafe { virtio.read_u32(1) },
            0x1,
            "device version mismatch"
        );
        let device_id = unsafe { virtio.read_u32(2) };
        // println!("subsystem device id: 0x{:x}", device_id);
        // println!("subsystem vendor id: 0x{:x}", unsafe { virtio.read_u32(3) });

        (virtio, VirtioMmioDevInfo { device_id })
    }

    unsafe fn read_u32(&self, offset: usize) -> u32 {
        unsafe { self.base.add(offset).read_volatile() }
    }

    unsafe fn write_u32(&self, offset: usize, value: u32) {
        unsafe { self.base.add(offset).write_volatile(value) }
    }
}

impl Debug for VirtioMmioDev {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "VirtioMmioDev@{:p}", self.base)
    }
}

pub struct Virtio9pDriver {
    dev: VirtioMmioDev,
}

impl Virtio9pDriver {
    pub fn new(dev: VirtioMmioDev) -> Self {
        Self { dev }
    }

    pub fn init(&self) {
        unsafe {
            self.dev.write_u32(0x14, 0x1); // Set status to acknowledge
            self.dev.write_u32(0x14, 0x5); // Set status to driver
        }
    }
}

impl Debug for Virtio9pDriver {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Virtio9pDriver@{:p}", self.dev.base)
    }
}
