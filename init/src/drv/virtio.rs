use alloc::vec;
use alloc::vec::Vec;
use core::ops::Deref;
use core::ptr::NonNull;
use fdt_rs::base::DevTree;
use fdt_rs::prelude::{FallibleIterator, PropReader};
use kernel_api::user::{
    dump_hex_slice, mem_unmap, mm_map, mm_unmap_range, phy_map, region_create_physical,
    virt_to_phys, AsciiStr,
};
use kernel_api::{println, PhyMapFlags};
use no_std_io::io::{Cursor, Read};
use virtio_drivers::device::virtio_9p::VirtIO9p;
use virtio_drivers::transport::mmio::{MmioError, MmioTransport, VirtIOHeader};
use virtio_drivers::transport::{DeviceType, DeviceTypeError, Transport};

pub struct VirtioNinep {
    ninep: VirtIO9p<HalImpl, MmioTransport<'static>>,
    mapped_header: AutoUnmap,
    req: Vec<u8>,
    res: Vec<u8>,
    next_fid: u32,
}

impl VirtioNinep {
    const MSIZE: u32 = 65 * 1024;
    pub const O_RDONLY: u32 = 0;
    pub const O_DIRECTORY: u32 = 1 << 14;

    pub fn find(dtb: &DevTree) -> Option<Self> {
        let mut nodes = dtb.compatible_nodes("virtio,mmio");
        while let Some(node) = nodes.next().expect("Failed to get next virtio node") {
            let mut prop_iter = node.props();
            while let Some(prop) = prop_iter.next().expect("Failed to get next property") {
                if prop.name().expect("Failed to get property name") == "reg" {
                    let reg = prop.u64(0).expect("Failed to get reg property");
                    let size = prop.u64(1).expect("Failed to get size property");
                    // println!("virtio: base: {reg:x} | size: {size:x}");
                    let offset = reg as usize % 0x1000;
                    let region = region_create_physical(0, reg as usize - offset, 0x1000)
                        .expect("failed to create region");
                    let mapped = AutoUnmap(
                        mm_map(None, &region, 0x1000).expect("failed to map virtio header"),
                        0x1000,
                    );
                    if mapped.is_null() {
                        panic!("virtio: header got mapped as null");
                    }
                    let header = unsafe {
                        NonNull::new(mapped.byte_offset(offset as isize) as *mut VirtIOHeader)
                            .unwrap()
                    };

                    match unsafe { MmioTransport::new(header, size as usize) } {
                        // Skip disconnected devices
                        Err(MmioError::InvalidDeviceID(DeviceTypeError::InvalidDeviceType(0))) => {
                            core::mem::forget(mapped); // FIXME: unmapping is broken
                            continue;
                        }
                        Err(e) => panic!("Error creating VirtIO MMIO transport: {}", e),
                        Ok(transport) => {
                            println!(
                                "Detected virtio MMIO device with vendor id {:#X}, device type {:?}, version {:?}",
                                transport.vendor_id(),
                                transport.device_type(),
                                transport.version(),
                            );

                            match transport.device_type() {
                                DeviceType::_9P => {
                                    let ninep = VirtIO9p::<HalImpl, MmioTransport>::new(transport)
                                        .expect("failed to create 9p driver");
                                    return Some(Self {
                                        ninep,
                                        mapped_header: mapped,
                                        req: Vec::with_capacity(Self::MSIZE as usize),
                                        res: vec![0u8; Self::MSIZE as usize],
                                        next_fid: 1,
                                    });
                                }
                                t => {
                                    core::mem::forget(mapped); // FIXME: unmapping is broken
                                    println!("Unrecognized virtio device: {t:?}")
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

    fn next_fid(&mut self) -> u32 {
        self.next_fid += 1;
        self.next_fid
    }

    pub fn init(&mut self) -> NinepFid {
        println!("virtio-9p: mount tag: {}", self.ninep.mount_tag());

        let rversion = NinepTversion { msize: Self::MSIZE }.do_req(
            &mut self.ninep,
            &mut self.req,
            &mut self.res,
        );
        if rversion.msize != Self::MSIZE {
            panic!(
                "virtio-9p: expected msize {}, got: {}",
                Self::MSIZE,
                rversion.msize
            );
        }
        assert_eq!(rversion.version, b"9P2000.L");
        drop(rversion);

        let root_fid = self.next_fid();
        NinepTattach {
            fid: root_fid,
            afid: 0xffff,
            uname: b"root",
            aname: b"",
            n_uname: 0,
        }
        .do_req(&mut self.ninep, &mut self.req, &mut self.res);

        NinepFid { fid: root_fid }
    }

    pub fn open_file(&mut self, fid: NinepFid, path: &[u8], flags: u32) -> NinepFile {
        let open_fid = self.next_fid();
        let path_segments = path.split(|&b| b == b'/').collect::<Vec<&[u8]>>();
        let mut path_segments = path_segments.as_slice();
        if path.is_empty() {
            path_segments = &[];
        }
        NinepTwalk {
            fid: fid.fid,
            new_fid: open_fid,
            path: path_segments,
        }
        .do_req(&mut self.ninep, &mut self.req, &mut self.res);
        NinepTlopen {
            fid: open_fid,
            flags: flags,
        }
        .do_req(&mut self.ninep, &mut self.req, &mut self.res);
        NinepFile { fid: open_fid }
    }

    pub fn read_dir(&mut self, dir: NinepFile, offset: u64) -> NinepReadDir {
        let data = NinepTreaddir {
            fid: dir.fid,
            offset: offset,
            count: Self::MSIZE - 24,
        }
        .do_req(&mut self.ninep, &mut self.req, &mut self.res)
        .data;
        println!("virtio-9p: read_dir data:");
        dump_hex_slice(&data);
        NinepReadDir {
            data: Cursor::new(data),
        }
    }

    pub fn read_file(&mut self, file: NinepFile, offset: u64, count: u32) -> Vec<u8> {
        NinepTread {
            fid: file.fid,
            offset: offset,
            count: count,
        }
        .do_req(&mut self.ninep, &mut self.req, &mut self.res)
        .data
    }

    pub fn close_fid(&mut self, fid: NinepFid) {
        NinepTclunk { fid: fid.fid }.do_req(&mut self.ninep, &mut self.req, &mut self.res);
    }

    pub fn close_file(&mut self, file: NinepFile) {
        NinepTclunk { fid: file.fid }.do_req(&mut self.ninep, &mut self.req, &mut self.res);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NinepFid {
    fid: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct NinepFile {
    fid: u32,
}

pub struct NinepReadDir {
    data: Cursor<Vec<u8>>,
}

impl NinepReadDir {
    pub fn is_empty(&self) -> bool {
        self.data.get_ref().is_empty()
    }
}

impl Iterator for NinepReadDir {
    type Item = NinepReadDirEntry;

    fn next(&mut self) -> Option<Self::Item> {
        if self.data.position() >= self.data.get_ref().len() as u64 {
            return None;
        }

        let _qid = Qid::ninep_read(&mut self.data);
        let offset = u64::ninep_read(&mut self.data);
        let ty = u8::ninep_read(&mut self.data);
        let name = Vec::<u8>::ninep_read(&mut self.data);
        Some(NinepReadDirEntry { offset, ty, name })
    }
}

pub struct NinepReadDirEntry {
    pub offset: u64,
    pub ty: u8,
    pub name: Vec<u8>,
}

impl core::fmt::Debug for NinepReadDirEntry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "NinepReadDirEntry {{ offset: {}, ty: {}, name: \"{}\" }}",
            self.offset,
            self.ty,
            AsciiStr(&self.name)
        )
    }
}

struct AutoUnmap(*mut (), usize);

impl Deref for AutoUnmap {
    type Target = *mut ();

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for AutoUnmap {
    fn drop(&mut self) {
        if self.1 > 0 {
            mm_unmap_range(None, self.0, self.1).expect("failed to unmap");
        }
    }
}

struct HalImpl;

unsafe impl virtio_drivers::Hal for HalImpl {
    fn dma_alloc(
        pages: usize,
        _direction: virtio_drivers::BufferDirection,
    ) -> (virtio_drivers::PhysAddr, NonNull<u8>) {
        unsafe {
            let (virt_addr, phy_addr) = phy_map(
                usize::MAX,
                pages * 0x1000,
                PhyMapFlags::ReadWrite | PhyMapFlags::DeviceMem,
            )
            .expect("failed to map dma memory");

            println!("virtio: dma_alloc: phy_addr: 0x{phy_addr:x} | virt_addr: {virt_addr:p}");
            (phy_addr, NonNull::new(virt_addr as *mut u8).unwrap())
        }
    }

    unsafe fn dma_dealloc(
        paddr: virtio_drivers::PhysAddr,
        vaddr: NonNull<u8>,
        pages: usize,
    ) -> i32 {
        println!("virtio: dma_dealloc paddr: {paddr:x} | vaddr: {vaddr:?} | pages: {pages}");
        mem_unmap(vaddr.as_ptr() as *const (), pages * 0x1000).expect("mem_unmap failed");
        0
    }

    unsafe fn mmio_phys_to_virt(paddr: virtio_drivers::PhysAddr, size: usize) -> NonNull<u8> {
        todo!("mmio_phys_to_virt paddr: {paddr:x} | size: {size}")
    }

    unsafe fn share(
        buffer: NonNull<[u8]>,
        _direction: virtio_drivers::BufferDirection,
    ) -> virtio_drivers::PhysAddr {
        virt_to_phys(buffer.as_ptr() as *const ()).expect("virt_to_phys failed")
    }

    unsafe fn unshare(
        _paddr: virtio_drivers::PhysAddr,
        _buffer: NonNull<[u8]>,
        _direction: virtio_drivers::BufferDirection,
    ) {
        // todo!("unshare paddr: {paddr:x} | buffer: {buffer:?} | direction: {direction:?}")
    }
}

#[allow(dead_code)]
#[derive(Debug, Copy, Clone)]
struct Qid {
    ty: u8,
    version: u32,
    path: u64,
}

trait NinepSerialize {
    fn ninep_measure(&self) -> u32;
    fn ninep_write(&self, req: &mut Vec<u8>);
}

trait NinepDeserialize {
    fn ninep_read<B: AsRef<[u8]>>(cur: &mut Cursor<B>) -> Self;
}

trait NinepReq: NinepSerialize {
    const TYPE: u8;
    type Res: NinepRes;

    fn do_req(
        &self,
        ninep: &mut VirtIO9p<HalImpl, MmioTransport>,
        req_buf: &mut Vec<u8>,
        res_buf: &mut [u8],
    ) -> Self::Res {
        req_buf.clear();
        (4u32 + 1 + 2 + self.ninep_measure()).ninep_write(req_buf);
        Self::TYPE.ninep_write(req_buf);
        0xffffu16.ninep_write(req_buf);
        self.ninep_write(req_buf);
        // println!("virtio-9p: do_req: sending req len: {}", req_buf.len());
        // dump_hex_slice(req_buf.as_slice());
        ninep
            .request(req_buf.as_slice(), res_buf)
            .expect("failed to request");
        let len = u32::from_le_bytes([res_buf[0], res_buf[1], res_buf[2], res_buf[3]]);
        // println!("virtio-9p: do_req: received len: {len}");
        let res = &res_buf[4..len as usize];
        // dump_hex_slice(res);
        let mut res = Cursor::new(res);
        let ty = u8::ninep_read(&mut res);
        if ty != Self::Res::TYPE {
            panic!("virtio-9p: expected {}, got: {ty}", Self::Res::TYPE);
        }
        let _tag = u16::ninep_read(&mut res);
        Self::Res::ninep_read(&mut res)
    }
}

trait NinepRes: NinepDeserialize {
    const TYPE: u8;
}

macro_rules! ninep_serde_num {
    ($ty:ty) => {
        impl NinepSerialize for $ty {
            fn ninep_measure(&self) -> u32 {
                core::mem::size_of::<$ty>() as u32
            }

            fn ninep_write(&self, req: &mut Vec<u8>) {
                req.extend_from_slice(&self.to_le_bytes());
            }
        }

        impl NinepDeserialize for $ty {
            fn ninep_read<B: AsRef<[u8]>>(cur: &mut Cursor<B>) -> Self {
                let mut val = [0u8; core::mem::size_of::<$ty>()];
                cur.read_exact(&mut val).expect("9p message too short");
                let val = Self::from_le_bytes(val.try_into().unwrap());
                val
            }
        }
    };
}

ninep_serde_num!(u8);
ninep_serde_num!(u16);
ninep_serde_num!(u32);
ninep_serde_num!(u64);

impl NinepSerialize for Qid {
    fn ninep_measure(&self) -> u32 {
        13
    }

    fn ninep_write(&self, req: &mut Vec<u8>) {
        self.ty.ninep_write(req);
        self.version.ninep_write(req);
        self.path.ninep_write(req);
    }
}

impl NinepDeserialize for Qid {
    fn ninep_read<B: AsRef<[u8]>>(cur: &mut Cursor<B>) -> Self {
        Self {
            ty: u8::ninep_read(cur),
            version: u32::ninep_read(cur),
            path: u64::ninep_read(cur),
        }
    }
}

impl NinepSerialize for &[u8] {
    fn ninep_measure(&self) -> u32 {
        2 + self.len() as u32
    }

    fn ninep_write(&self, req: &mut Vec<u8>) {
        (self.len() as u16).ninep_write(req);
        req.extend_from_slice(self);
    }
}

impl NinepDeserialize for Vec<u8> {
    fn ninep_read<B: AsRef<[u8]>>(cur: &mut Cursor<B>) -> Self {
        let len = u16::ninep_read(cur) as usize;
        println!("virtio-9p: ninep_read: vec len: {len}");
        let mut val = vec![0u8; len];
        cur.read_exact(&mut val).expect("9p message too short");
        val
    }
}

struct NinepTversion {
    msize: u32,
}

impl NinepSerialize for NinepTversion {
    fn ninep_measure(&self) -> u32 {
        4 + 2 + 8
    }

    fn ninep_write(&self, req: &mut Vec<u8>) {
        self.msize.ninep_write(req);
        8u16.ninep_write(req);
        req.extend_from_slice(b"9P2000.L");
    }
}

impl NinepReq for NinepTversion {
    const TYPE: u8 = 100;
    type Res = NinepRversion;
}

struct NinepRversion {
    msize: u32,
    version: Vec<u8>,
}

impl NinepDeserialize for NinepRversion {
    fn ninep_read<B: AsRef<[u8]>>(cur: &mut Cursor<B>) -> Self {
        let msize = u32::ninep_read(cur);
        let version = Vec::<u8>::ninep_read(cur);
        Self { msize, version }
    }
}

impl NinepRes for NinepRversion {
    const TYPE: u8 = 101;
}

struct NinepTwalk<'a> {
    fid: u32,
    new_fid: u32,
    path: &'a [&'a [u8]],
}

impl<'a> NinepSerialize for NinepTwalk<'a> {
    fn ninep_measure(&self) -> u32 {
        self.fid.ninep_measure()
            + self.new_fid.ninep_measure()
            + self.path.len() as u32 * 2
            + self.path.iter().map(|name| name.len() as u32).sum::<u32>()
    }

    fn ninep_write(&self, req: &mut Vec<u8>) {
        self.fid.ninep_write(req);
        self.new_fid.ninep_write(req);
        (self.path.len() as u16).ninep_write(req);
        for name in self.path {
            name.ninep_write(req);
        }
    }
}

impl<'a> NinepReq for NinepTwalk<'a> {
    const TYPE: u8 = 110;
    type Res = NinepRwalk;
}

struct NinepRwalk {
    nwqid: u16,
    qids: [Qid; 16],
}

impl NinepDeserialize for NinepRwalk {
    fn ninep_read<B: AsRef<[u8]>>(cur: &mut Cursor<B>) -> Self {
        let nwqid = u16::ninep_read(cur);
        let mut qids = [Qid {
            ty: 0,
            version: 0,
            path: 0,
        }; 16];
        for i in 0..nwqid as usize {
            qids[i] = Qid::ninep_read(cur);
        }
        Self { nwqid, qids }
    }
}

impl NinepRes for NinepRwalk {
    const TYPE: u8 = 111;
}

struct NinepTread {
    fid: u32,
    offset: u64,
    count: u32,
}

impl NinepSerialize for NinepTread {
    fn ninep_measure(&self) -> u32 {
        self.fid.ninep_measure() + self.offset.ninep_measure() + self.count.ninep_measure()
    }

    fn ninep_write(&self, req: &mut Vec<u8>) {
        self.fid.ninep_write(req);
        self.offset.ninep_write(req);
        self.count.ninep_write(req);
    }
}

impl NinepReq for NinepTread {
    const TYPE: u8 = 116;
    type Res = NinepRread;
}

struct NinepRread {
    data: Vec<u8>,
}

impl NinepDeserialize for NinepRread {
    fn ninep_read<B: AsRef<[u8]>>(cur: &mut Cursor<B>) -> Self {
        let len = u32::ninep_read(cur);
        let mut data = vec![0u8; len as usize];
        cur.read_exact(&mut data).expect("9p message too short");
        Self { data }
    }
}

impl NinepRes for NinepRread {
    const TYPE: u8 = 117;
}

struct NinepTwrite<'a> {
    fid: u32,
    offset: u64,
    data: &'a [u8],
}

impl<'a> NinepSerialize for NinepTwrite<'a> {
    fn ninep_measure(&self) -> u32 {
        self.fid.ninep_measure() + self.offset.ninep_measure() + self.data.ninep_measure()
    }

    fn ninep_write(&self, req: &mut Vec<u8>) {
        self.fid.ninep_write(req);
        self.offset.ninep_write(req);
        self.data.ninep_write(req);
    }
}

impl<'a> NinepReq for NinepTwrite<'a> {
    const TYPE: u8 = 118;
    type Res = NinepRwrite;
}

struct NinepRwrite {
    count: u32,
}

impl NinepDeserialize for NinepRwrite {
    fn ninep_read<B: AsRef<[u8]>>(cur: &mut Cursor<B>) -> Self {
        let count = u32::ninep_read(cur);
        Self { count }
    }
}

impl NinepRes for NinepRwrite {
    const TYPE: u8 = 119;
}

struct NinepTclunk {
    fid: u32,
}

impl NinepSerialize for NinepTclunk {
    fn ninep_measure(&self) -> u32 {
        self.fid.ninep_measure()
    }

    fn ninep_write(&self, req: &mut Vec<u8>) {
        self.fid.ninep_write(req);
    }
}

impl NinepReq for NinepTclunk {
    const TYPE: u8 = 120;
    type Res = NinepRclunk;
}

struct NinepRclunk {}

impl NinepDeserialize for NinepRclunk {
    fn ninep_read<B: AsRef<[u8]>>(_cur: &mut Cursor<B>) -> Self {
        Self {}
    }
}

impl NinepRes for NinepRclunk {
    const TYPE: u8 = 121;
}

struct NinepTremove {
    fid: u32,
}

impl NinepSerialize for NinepTremove {
    fn ninep_measure(&self) -> u32 {
        self.fid.ninep_measure()
    }

    fn ninep_write(&self, req: &mut Vec<u8>) {
        self.fid.ninep_write(req);
    }
}

impl NinepReq for NinepTremove {
    const TYPE: u8 = 122;
    type Res = NinepRremove;
}

struct NinepRremove {}

impl NinepDeserialize for NinepRremove {
    fn ninep_read<B: AsRef<[u8]>>(_cur: &mut Cursor<B>) -> Self {
        Self {}
    }
}

impl NinepRes for NinepRremove {
    const TYPE: u8 = 123;
}

struct NinepTattach<'a> {
    fid: u32,
    afid: u32,
    uname: &'a [u8],
    aname: &'a [u8],
    n_uname: u32,
}

impl<'a> NinepSerialize for NinepTattach<'a> {
    fn ninep_measure(&self) -> u32 {
        self.fid.ninep_measure()
            + self.afid.ninep_measure()
            + self.uname.ninep_measure()
            + self.aname.ninep_measure()
            + self.n_uname.ninep_measure()
    }

    fn ninep_write(&self, req: &mut Vec<u8>) {
        self.fid.ninep_write(req);
        self.afid.ninep_write(req);
        self.uname.ninep_write(req);
        self.aname.ninep_write(req);
        self.n_uname.ninep_write(req);
    }
}

impl<'a> NinepReq for NinepTattach<'a> {
    const TYPE: u8 = 104;
    type Res = NinepRattach;
}

struct NinepRattach {
    qid: Qid,
}

impl NinepDeserialize for NinepRattach {
    fn ninep_read<B: AsRef<[u8]>>(cur: &mut Cursor<B>) -> Self {
        let qid = Qid::ninep_read(cur);
        Self { qid }
    }
}

impl NinepRes for NinepRattach {
    const TYPE: u8 = 105;
}

struct NinepTstatfs {
    fid: u32,
}

impl NinepSerialize for NinepTstatfs {
    fn ninep_measure(&self) -> u32 {
        self.fid.ninep_measure()
    }

    fn ninep_write(&self, req: &mut Vec<u8>) {
        self.fid.ninep_write(req);
    }
}

impl NinepReq for NinepTstatfs {
    const TYPE: u8 = 8;
    type Res = NinepRstatfs;
}

struct NinepRstatfs {
    ty: u32,
    bsize: u32,
    blocks: u64,
    bfree: u64,
    bavail: u64,
    files: u64,
    ffree: u64,
    fsid: u64,
    namelen: u32,
}

impl NinepDeserialize for NinepRstatfs {
    fn ninep_read<B: AsRef<[u8]>>(cur: &mut Cursor<B>) -> Self {
        let ty = u32::ninep_read(cur);
        let bsize = u32::ninep_read(cur);
        let blocks = u64::ninep_read(cur);
        let bfree = u64::ninep_read(cur);
        let bavail = u64::ninep_read(cur);
        let files = u64::ninep_read(cur);
        let ffree = u64::ninep_read(cur);
        let fsid = u64::ninep_read(cur);
        let namelen = u32::ninep_read(cur);
        Self {
            ty,
            bsize,
            blocks,
            bfree,
            bavail,
            files,
            ffree,
            fsid,
            namelen,
        }
    }
}

impl NinepRes for NinepRstatfs {
    const TYPE: u8 = 9;
}

struct NinepTlopen {
    fid: u32,
    flags: u32,
}

impl NinepSerialize for NinepTlopen {
    fn ninep_measure(&self) -> u32 {
        self.fid.ninep_measure() + self.flags.ninep_measure()
    }

    fn ninep_write(&self, req: &mut Vec<u8>) {
        self.fid.ninep_write(req);
        self.flags.ninep_write(req);
    }
}

impl NinepReq for NinepTlopen {
    const TYPE: u8 = 12;
    type Res = NinepRlopen;
}

struct NinepRlopen {
    qid: Qid,
    iounit: u32,
}

impl NinepDeserialize for NinepRlopen {
    fn ninep_read<B: AsRef<[u8]>>(cur: &mut Cursor<B>) -> Self {
        let qid = Qid::ninep_read(cur);
        let iounit = u32::ninep_read(cur);
        Self { qid, iounit }
    }
}

impl NinepRes for NinepRlopen {
    const TYPE: u8 = 13;
}

struct NinepTreaddir {
    fid: u32,
    offset: u64,
    count: u32,
}

impl NinepSerialize for NinepTreaddir {
    fn ninep_measure(&self) -> u32 {
        self.fid.ninep_measure() + self.offset.ninep_measure() + self.count.ninep_measure()
    }

    fn ninep_write(&self, req: &mut Vec<u8>) {
        self.fid.ninep_write(req);
        self.offset.ninep_write(req);
        self.count.ninep_write(req);
    }
}

impl NinepReq for NinepTreaddir {
    const TYPE: u8 = 40;
    type Res = NinepRreaddir;
}

struct NinepRreaddir {
    data: Vec<u8>,
}

impl NinepDeserialize for NinepRreaddir {
    fn ninep_read<B: AsRef<[u8]>>(cur: &mut Cursor<B>) -> Self {
        let count = u32::ninep_read(cur);
        println!("virtio-9p: ninep_read: readdir count: {count}");
        let mut data = vec![0u8; count as usize];
        cur.read_exact(&mut data).expect("9p message too short");
        Self { data }
    }
}

impl NinepRes for NinepRreaddir {
    const TYPE: u8 = 41;
}
