use crate::println;
use crate::utils::{mem_unmap, phy_map};
use fdt_rs::base::DevTree;
use fdt_rs::error::DevTreeError;
use fdt_rs::prelude::{FallibleIterator, PropReader};
use kernel_api::PhyMapFlags;

pub struct QemuFwCfg {
    base: *mut u64,
}

impl QemuFwCfg {
    pub fn find_and_init(dtb: &DevTree) -> Result<Self, DevTreeError> {
        let mut fwcfg_nodes = dtb.compatible_nodes("qemu,fw-cfg-mmio");
        let Some(fwcfg_node) = fwcfg_nodes.next()? else {
            panic!("FWCFG node not found");
        };
        let mut prop_iter = fwcfg_node.props();
        while let Some(prop) = prop_iter.next()? {
            if prop.name()? == "reg" {
                let reg = prop.u64(0)?;
                let size = prop.u64(1)?;
                println!("qemu_fwcfg: base: {reg:x} | size: {size:x}");
                let base = unsafe {
                    phy_map(
                        reg as usize,
                        0x1000,
                        PhyMapFlags::ReadWrite | PhyMapFlags::DeviceMem,
                    )
                    .expect("failed to map fwcfg")
                };
                return Ok(Self {
                    base: base as *mut u64,
                });
            }
        }
        unreachable!("FWCFG node found but no reg property");
    }

    fn seek(&self, selector: u16) {
        unsafe {
            let selector_ptr = self.base.add(1) as *mut u16;
            selector_ptr.write_volatile(selector.to_be());
        }
    }

    fn read_u64(&self) -> u64 {
        unsafe { self.base.read_volatile() }
    }

    fn read_u32(&self) -> u32 {
        unsafe { (self.base as *const u32).read_volatile() }
    }

    fn seek_read_u64(&self, selector: u16) -> u64 {
        self.seek(selector);
        self.read_u64()
    }

    fn seek_read_u32(&self, selector: u16) -> u32 {
        self.seek(selector);
        self.read_u32()
    }

    pub fn files(&mut self) -> QemuFwCfgFiles<'_> {
        let len = self.seek_read_u32(0x19).to_be();
        QemuFwCfgFiles {
            fwcfg: self,
            len,
            i: 0,
        }
    }

    pub fn dump_files(&mut self) {
        let len = self.files().len;
        for i in 0..len {
            let file = self.files().nth(i as usize).unwrap();
            println!(
                "qemu_fwcfg: {} | size: {} | select: 0x{:x}",
                file.name(),
                file.size,
                file.selector
            );

            let QemuFwCfgSelector { size, selector, .. } = file;
            self.seek(selector);
            let mut size = size.min(32);
            while size >= 8 {
                println!("qemu_fwcfg:   {:016x}", self.read_u64().to_be());
                size -= 8;
            }
            if size > 0 {
                println!("qemu_fwcfg:   {:016x}", self.read_u64().to_be());
            }
        }
    }
}

impl Drop for QemuFwCfg {
    fn drop(&mut self) {
        unsafe {
            mem_unmap(self.base as _, 0x1000).expect("failed to unmap fwcfg");
        }
    }
}

pub struct QemuFwCfgFiles<'a> {
    fwcfg: &'a mut QemuFwCfg,
    len: u32,
    i: u32,
}

impl<'a> Iterator for QemuFwCfgFiles<'a> {
    type Item = QemuFwCfgSelector;
    fn next(&mut self) -> Option<Self::Item> {
        if self.i >= self.len {
            return None;
        }

        let size = self.fwcfg.read_u32().to_be();
        let selector = (self.fwcfg.read_u32() as u16).to_be();
        let mut name_buf = [0u64; 7];
        for slot in name_buf.iter_mut() {
            *slot = self.fwcfg.read_u64();
        }
        let name: [u8; 56] = zerocopy::transmute!(name_buf);
        self.i += 1;
        Some(QemuFwCfgSelector {
            idx: self.i - 1,
            size,
            selector,
            name,
        })
    }
}

pub struct QemuFwCfgSelector {
    pub idx: u32,
    pub size: u32,
    pub selector: u16,
    name: [u8; 56],
}

impl QemuFwCfgSelector {
    pub fn name(&self) -> &str {
        let name = core::str::from_utf8(&self.name).expect("invalid UTF-8 in FWCFG name");
        name.split_once('\0').unwrap_or((name, "")).0
    }
}
