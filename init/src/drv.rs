use fdt_rs::{
    base::DevTree,
    error::DevTreeError,
    prelude::{FallibleIterator, PropReader},
};
use kernel_api::kernel_device;

use crate::{get_msr, println, utils::load_kernel_device};

#[allow(dead_code)]
pub struct GicAndTimer {
    gicd_base: u64,
    gicc_base: u64,
    timer_ppi_interrupt: u32,
}

impl GicAndTimer {
    pub fn find_and_init(dtb: &DevTree) -> Result<Self, DevTreeError> {
        let mut gicd_base = 0;
        let mut gicc_base = 0;
        const PPI_OFFSET: u32 = 16;
        const PPI_NON_SECURE_PHYS_TIMER: u32 = 14;

        println!("Extracting timer information from DTB");

        // Interrupt node
        let mut intc_nodes = dtb.compatible_nodes("arm,cortex-a15-gic");
        let Some(intc_node) = intc_nodes.next()? else {
            panic!("Interrupt controller node not found");
        };
        let mut prop_iter = intc_node.props();
        while let Some(prop) = prop_iter.next()? {
            let name = prop.name()?;
            if name == "reg" {
                gicd_base = prop.u64(0)?;
                // gicd_size = prop.u64(1)?;
                gicc_base = prop.u64(2)?;
                // gicc_size = prop.u64(3)?;
            } else if name == "#size-cells" {
                assert_eq!(
                    prop.u32(0)?,
                    2,
                    "Interrupt controller node must have #size-cells = 2"
                );
            } else if name == "#address-cells" {
                assert_eq!(
                    prop.u32(0)?,
                    2,
                    "Interrupt controller node must have #address-cells = 2"
                );
            } else if name == "#interrupt-cells" {
                assert_eq!(
                    prop.u32(0)?,
                    3,
                    "Interrupt controller node must have #interrupt-cells = 3"
                );
            }
        }
        assert!(
            intc_nodes.next()?.is_none(),
            "Multiple interrupt controller nodes found"
        );

        // Timer node
        let mut timer_nodes = dtb.compatible_nodes("arm,armv8-timer");
        let Some(timer_node) = timer_nodes.next()? else {
            panic!("Timer node not found");
        };
        let mut prop_iter = timer_node.props();
        while let Some(prop) = prop_iter.next()? {
            let name = prop.name()?;
            if name == "interrupts" {
                assert_eq!(prop.length(), 48, "Interrupts property must be 48 bytes");
                let int_type = prop.u32(3)?;
                assert_eq!(int_type, 1, "Only PPI interrupts are supported");
                let ppi_id = prop.u32(4)?;
                assert_eq!(
                    ppi_id, PPI_NON_SECURE_PHYS_TIMER,
                    "PPI ID must be {PPI_NON_SECURE_PHYS_TIMER}"
                );
                let flags = prop.u32(5)?;
                assert_eq!(flags & 0x4, 0x4, "Interrupt must be level-triggered");
            }
        }
        assert!(timer_nodes.next()?.is_none(), "Multiple timer nodes found");

        // Load the interrupt controller and timer info into the kernel
        let timer_ppi_interrupt = PPI_OFFSET + PPI_NON_SECURE_PHYS_TIMER;
        unsafe {
            load_kernel_device(&kernel_device::GicAndTimer {
                gicd_base,
                gicc_base,
                timer_ppi_interrupt,
                _padding: 0,
            })
        }
        .expect("Failed to load kernel device");

        Ok(GicAndTimer {
            gicd_base,
            gicc_base,
            timer_ppi_interrupt,
        })
    }

    pub fn current_time_ms() -> u64 {
        unsafe {
            let ticks = get_msr!(cntpct_el0);
            let freq = get_msr!(cntfrq_el0);
            (ticks * 1000) / freq
        }
    }
}
