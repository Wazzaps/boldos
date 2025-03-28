// let mut node_iter = dtb.compatible_nodes("qemu,fw-cfg-mmio");
// while let Some(node) = node_iter.next().unwrap() {
//     println!("{}", node.name().unwrap());
//     let mut prop_iter = node.props();
//     while let Some(prop) = prop_iter.next().unwrap() {
//         println!("  {}: {:?}", prop.name().unwrap(), prop.iter_str());
//     }
// }

// struct QemuFwCfg {
//     base: *mut u64,
// }
//
// impl QemuFwCfg {
//     fn new(base: *mut u64) -> Self {
//         let fw_cfg = Self { base };
//         assert_eq!(fw_cfg.seek_read(0), 0x554d4551, "QEMU signature mismatch");
//         fw_cfg
//     }
//
//     fn seek(&self, selector: u16) {
//         unsafe {
//             let selector_ptr = self.base.add(1) as *mut u16;
//             selector_ptr.write_volatile(selector.to_be());
//         }
//     }
//
//     fn read(&self) -> u64 {
//         unsafe { self.base.read_volatile() }
//     }
//
//     fn read_u32(&self) -> u32 {
//         unsafe { (self.base as *const u32).read_volatile() }
//     }
//
//     fn seek_read(&self, selector: u16) -> u64 {
//         self.seek(selector);
//         self.read()
//     }
//
//     fn seek_read_u32(&self, selector: u16) -> u32 {
//         self.seek(selector);
//         self.read_u32()
//     }
// }
// fn fw_cfg_experiment() {
// let fw_cfg = QemuFwCfg::new(0x9020000 as *mut u64);
// let len = fw_cfg.seek_read_u32(0x19).to_be();
// println!("file dir size = {:x}", len);
// for i in 0..len {
//     println!("{}:", i);
//     println!("  size: {}", fw_cfg.read_u32().to_be());
//     println!("  select: 0x{:x}", (fw_cfg.read_u32() as u16).to_be());
//     let mut name_buf = [0u64; 7];
//     for slot in name_buf.iter_mut() {
//         *slot = fw_cfg.read();
//     }
//     let name: [u8; 56] = zerocopy::transmute!(name_buf);
//     let name = core::str::from_utf8(&name).unwrap();
//     println!(
//         "  name: {:?}",
//         name.split_once('\0').unwrap_or((name, "")).0
//     );
// }
//
// for select in 0x0..0x20 {
//     println!("  0x{:x}: {:x}", select, fw_cfg.seek_read(select));
//     // fw_cfg.seek(select);
//     // let mut buf = [0u32; 4];
//     // for slot in buf.iter_mut() {
//     //     *slot = fw_cfg.read_u32();
//     // }
//     // let name: [u8; 16] = zerocopy::transmute!(buf);
//     // println!("  0x{:x}: {:?}", select, &name);
// }
// }
