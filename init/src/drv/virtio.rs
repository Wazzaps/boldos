use crate::println;
use crate::utils::{dump_hex_slice, mem_unmap, phy_map, virt_to_phys};
use alloc::vec;
use alloc::vec::Vec;
use core::ptr::NonNull;
use fdt_rs::base::DevTree;
use fdt_rs::prelude::{FallibleIterator, PropReader};
use kernel_api::PhyMapFlags;
use virtio_drivers::device::blk::{VirtIOBlk, SECTOR_SIZE};
use virtio_drivers::device::virtio_9p::VirtIO9p;
use virtio_drivers::transport::mmio::{MmioError, MmioTransport, VirtIOHeader};
use virtio_drivers::transport::{DeviceType, DeviceTypeError, Transport};

pub fn virtio_experiment(dtb: &DevTree) {
    let mut nodes = dtb.compatible_nodes("virtio,mmio");
    while let Some(node) = nodes.next().expect("Failed to get next virtio node") {
        let mut prop_iter = node.props();
        while let Some(prop) = prop_iter.next().expect("Failed to get next property") {
            if prop.name().expect("Failed to get property name") == "reg" {
                let reg = prop.u64(0).expect("Failed to get reg property");
                let size = prop.u64(1).expect("Failed to get size property");
                // println!("virtio: base: {reg:x} | size: {size:x}");
                let offset = reg as usize % 0x1000;
                let (header, _) = unsafe {
                    phy_map(
                        reg as usize - offset,
                        0x1000,
                        PhyMapFlags::ReadWrite | PhyMapFlags::DeviceMem,
                    )
                    .expect("failed to map virtio header")
                };
                if header.is_null() {
                    panic!("virtio: header got mapped as null");
                }
                let header = unsafe {
                    NonNull::new(header.byte_offset(offset as isize) as *mut VirtIOHeader).unwrap()
                };

                match unsafe { MmioTransport::new(header, size as usize) } {
                    // Skip disconnected devices
                    Err(MmioError::InvalidDeviceID(DeviceTypeError::InvalidDeviceType(0))) => {
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
                            DeviceType::Block => virtio_blk(transport),
                            //     DeviceType::GPU => virtio_gpu(transport),
                            //     DeviceType::Network => virtio_net(transport),
                            //     DeviceType::Console => virtio_console(transport),
                            //     DeviceType::Socket => match virtio_socket(transport) {
                            //         Ok(()) => info!("virtio-socket test finished successfully"),
                            //         Err(e) => error!("virtio-socket test finished with error '{e:?}'"),
                            //     },
                            DeviceType::_9P => virtio_9p(transport),
                            //     DeviceType::EntropySource => virtio_rng(transport),
                            t => println!("Unrecognized virtio device: {t:?}"),
                        }
                    }
                }
            }
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

fn virtio_blk(transport: MmioTransport) {
    let mut blk =
        VirtIOBlk::<HalImpl, MmioTransport>::new(transport).expect("failed to create blk driver");
    let capacity = blk.capacity();
    println!("virtio-blk: capacity in sectors: {capacity}");
    let mut buf = [0u8; SECTOR_SIZE];
    blk.read_blocks(0, &mut buf).expect("failed to read blocks");
    println!("virtio-blk: read sector: {buf:02x?}");
}

fn virtio_9p(transport: MmioTransport) {
    const MSIZE: u32 = 16384;

    let mut ninep =
        VirtIO9p::<HalImpl, MmioTransport>::new(transport).expect("failed to create 9p driver");
    println!("virtio-9p: mount tag: {}", ninep.mount_tag());

    let mut req = Vec::with_capacity(MSIZE as usize);
    let mut res = vec![0u8; MSIZE as usize];

    do_9p_tversion(MSIZE, &mut ninep, &mut req, &mut res);
    let root_qid = do_9p_tattach(1, &mut ninep, &mut req, &mut res);
    println!("virtio-9p: root qid: {root_qid:x?}");
    const O_RDONLY: u32 = 0;
    const O_DIRECTORY: u32 = 1 << 14;
    let root_lopen = do_9p_lopen(1, O_RDONLY | O_DIRECTORY, &mut ninep, &mut req, &mut res);
    println!("virtio-9p: root lopen qid: {root_lopen:x?}");
    do_9p_treaddir(1, 0, MSIZE - 24, &mut ninep, &mut req, &mut res);
}

#[allow(dead_code)]
#[derive(Debug, Copy, Clone)]
struct Qid {
    ty: u8,
    version: u32,
    path: u64,
}

fn do_9p_tversion(
    msize: u32,
    ninep: &mut VirtIO9p<HalImpl, MmioTransport>,
    req: &mut Vec<u8>,
    res: &mut [u8],
) {
    req.clear();
    // length
    req.extend_from_slice(&((4u32 + 1 + 2 + 4 + 2 + 8).to_le_bytes()));
    // type (Tversion)
    req.extend_from_slice(&[100u8]);
    // tag
    req.extend_from_slice(&(0xffffu16.to_le_bytes()));
    // Tversion.msize
    req.extend_from_slice(&(msize.to_le_bytes()));
    // Tversion.version
    req.extend_from_slice(&(8u16.to_le_bytes()));
    req.extend_from_slice(b"9P2000.L");

    println!("virtio-9p: sending Tversion:");
    dump_hex_slice(req.as_slice());
    ninep
        .request(req.as_slice(), res)
        .expect("failed to request");
    println!("virtio-9p: received Rversion:");
    let len = u32::from_le_bytes([res[0], res[1], res[2], res[3]]);
    dump_hex_slice(&res[..len as usize]);

    let res = &res[4..len as usize];
    let ty = res[0];
    if ty != 101 {
        panic!("virtio-9p: expected Rversion, got: {ty}");
    }
    let _tag = u16::from_le_bytes([res[1], res[2]]);
    let got_msize = u32::from_le_bytes([res[3], res[4], res[5], res[6]]);
    assert_eq!(got_msize, msize);
    let version_len = u16::from_le_bytes([res[7], res[8]]) as usize;
    let version = &res[9..9 + version_len];
    assert_eq!(version, b"9P2000.L");
}

fn do_9p_tattach(
    fid: u32,
    ninep: &mut VirtIO9p<HalImpl, MmioTransport>,
    req: &mut Vec<u8>,
    res: &mut [u8],
) -> Qid {
    req.clear();
    // length
    req.extend_from_slice(&((4u32 + 1 + 2 + 4 + 4 + 2 + 4 + 2 + 4).to_le_bytes()));
    // type (Tattach)
    req.extend_from_slice(&[104u8]);
    // tag
    req.extend_from_slice(&(1u16.to_le_bytes()));
    // Tattach.fid
    req.extend_from_slice(&(fid.to_le_bytes()));
    // Tattach.afid
    req.extend_from_slice(&(0xffffu32.to_le_bytes()));
    // Tattach.uname
    req.extend_from_slice(&(4u16.to_le_bytes()));
    req.extend_from_slice(b"root");
    // Tattach.aname
    req.extend_from_slice(&(0u16.to_le_bytes()));
    // req.extend_from_slice(b"");
    // Tattach.n_uname
    req.extend_from_slice(&(0u32.to_le_bytes()));

    println!("virtio-9p: sending Tattach:");
    dump_hex_slice(req.as_slice());
    ninep
        .request(req.as_slice(), res)
        .expect("failed to request");
    println!("virtio-9p: received Rattach:");
    let len = u32::from_le_bytes([res[0], res[1], res[2], res[3]]);
    dump_hex_slice(&res[..len as usize]);

    let res = &res[4..len as usize];
    let ty = res[0];
    if ty != 105 {
        panic!("virtio-9p: expected Rattach, got: {ty}");
    }
    let _tag = u16::from_le_bytes([res[1], res[2]]);
    let qid_ty = res[3];
    let qid_version = u32::from_le_bytes([res[4], res[5], res[6], res[7]]);
    let qid_path = u64::from_le_bytes([
        res[8], res[9], res[10], res[11], res[12], res[13], res[14], res[15],
    ]);
    Qid {
        ty: qid_ty,
        version: qid_version,
        path: qid_path,
    }
}

fn do_9p_lopen(
    fid: u32,
    flags: u32,
    ninep: &mut VirtIO9p<HalImpl, MmioTransport>,
    req: &mut Vec<u8>,
    res: &mut [u8],
) -> Qid {
    req.clear();
    // length
    req.extend_from_slice(&((4u32 + 1 + 2 + 4 + 4).to_le_bytes()));
    // type (Tlopen)
    req.extend_from_slice(&[12u8]);
    // tag
    req.extend_from_slice(&(1u16.to_le_bytes()));
    // Tlopen.fid
    req.extend_from_slice(&(fid.to_le_bytes()));
    // Tlopen.flags
    req.extend_from_slice(&(flags.to_le_bytes()));

    println!("virtio-9p: sending Tlopen:");
    dump_hex_slice(req.as_slice());
    ninep
        .request(req.as_slice(), res)
        .expect("failed to request");
    println!("virtio-9p: received Rlopen:");
    let len = u32::from_le_bytes([res[0], res[1], res[2], res[3]]);
    dump_hex_slice(&res[..len as usize]);
    let res = &res[4..len as usize];

    let ty = res[0];
    if ty != 13 {
        panic!("virtio-9p: expected Rlopen, got: {ty}");
    }
    let _tag = u16::from_le_bytes([res[1], res[2]]);
    let qid_ty = res[3];
    let qid_version = u32::from_le_bytes([res[4], res[5], res[6], res[7]]);
    let qid_path = u64::from_le_bytes([
        res[8], res[9], res[10], res[11], res[12], res[13], res[14], res[15],
    ]);
    let qid = Qid {
        ty: qid_ty,
        version: qid_version,
        path: qid_path,
    };
    let iounit = u32::from_le_bytes([res[16], res[17], res[18], res[19]]);
    println!("virtio-9p: lopen iounit: {iounit}");
    qid
}

fn do_9p_treaddir(
    fid: u32,
    offset: u64,
    count: u32,
    ninep: &mut VirtIO9p<HalImpl, MmioTransport>,
    req: &mut Vec<u8>,
    res: &mut [u8],
) {
    req.clear();
    // length
    req.extend_from_slice(&((4u32 + 1 + 2 + 4 + 8 + 4).to_le_bytes()));
    // type (Treaddir)
    req.extend_from_slice(&[40u8]);
    // tag
    req.extend_from_slice(&(1u16.to_le_bytes()));
    // Treaddir.fid
    req.extend_from_slice(&(fid.to_le_bytes()));
    // Treaddir.offset
    req.extend_from_slice(&(offset.to_le_bytes()));
    // Treaddir.count
    req.extend_from_slice(&(count.to_le_bytes()));

    println!("virtio-9p: sending Treaddir:");
    dump_hex_slice(req.as_slice());
    ninep
        .request(req.as_slice(), res)
        .expect("failed to request");
    println!("virtio-9p: received Rreaddir:");
    let len = u32::from_le_bytes([res[0], res[1], res[2], res[3]]);
    dump_hex_slice(&res[..len as usize]);

    let res = &res[4..len as usize];
    let ty = res[0];
    if ty != 41 {
        panic!("virtio-9p: expected Rreaddir, got: {ty}");
    }
    let _tag = u16::from_le_bytes([res[1], res[2]]);
    let entries_size = u32::from_le_bytes([res[3], res[4], res[5], res[6]]);
    println!("virtio-9p: entries_size: {entries_size}");
    let mut entries = &res[7..7 + entries_size as usize];
    while entries.len() > 0 {
        let qid_ty = entries[0];
        let qid_version = u32::from_le_bytes([entries[1], entries[2], entries[3], entries[4]]);
        let qid_path = u64::from_le_bytes([
            entries[5],
            entries[6],
            entries[7],
            entries[8],
            entries[9],
            entries[10],
            entries[11],
            entries[12],
        ]);
        let qid = Qid {
            ty: qid_ty,
            version: qid_version,
            path: qid_path,
        };
        let offset = u64::from_le_bytes([
            entries[13],
            entries[14],
            entries[15],
            entries[16],
            entries[17],
            entries[18],
            entries[19],
            entries[20],
        ]);
        let ty = entries[21];
        let name_len = u16::from_le_bytes([entries[22], entries[23]]);
        let name = &entries[24..24 + name_len as usize];
        if let Ok(name_utf8) = str::from_utf8(name) {
            println!("9p dir entry: {qid:x?} | offset: {offset} | type: {ty} | name: {name_utf8}");
        } else {
            println!("9p dir entry: {qid:x?} | offset: {offset} | type: {ty} | name: {name:?}");
        }
        entries = &entries[24 + name_len as usize..];
    }
}
