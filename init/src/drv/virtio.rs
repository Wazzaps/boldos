use crate::println;
use crate::utils::{mem_unmap, phy_map, virt_to_phys};
use core::ptr::NonNull;
use fdt_rs::base::DevTree;
use fdt_rs::prelude::{FallibleIterator, PropReader};
use kernel_api::PhyMapFlags;
use virtio_drivers::device::blk::{VirtIOBlk, SECTOR_SIZE};
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
                            // DeviceType::_9P => virtio_9p(transport),
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
