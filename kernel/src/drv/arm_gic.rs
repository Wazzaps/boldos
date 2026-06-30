use core::{
    arch::asm,
    ptr::{read_volatile, write_volatile},
};

use crate::{page_alloc::PhyAddr, println};

const GICD_BASE: usize = 0x08000000;
const GICC_BASE: usize = 0x08010000;

const GICD_CTLR: usize = 0x000;
const GICD_ISENABLER0: usize = 0x100;
const GICD_IPRIORITYR7: usize = 0x41C; // (30 / 4) * 4 = 0x41C

const GICC_CTLR: usize = 0x000;
const GICC_PMR: usize = 0x004;
const GICC_IAR: usize = 0x00C;
const GICC_EOIR: usize = 0x010;

unsafe fn mmio_write(base: usize, offset: usize, val: u32) {
    write_volatile(PhyAddr(base + offset).virt_dev_mut::<u32>(), val);
}

unsafe fn mmio_read(base: usize, offset: usize) -> u32 {
    read_volatile(PhyAddr(base + offset).virt_dev::<u32>())
}

pub unsafe fn init_gic() {
    println!("  drv: Initializing ARM GIC");

    // -- distributor setup --

    // Disable distributor during configuration
    mmio_write(GICD_BASE, GICD_CTLR, 0);

    // Configure PPI 30 (Non-Secure Physical Timer)
    // Enable register 0 handles IRQs 0-31. Set bit 30.
    let enable_bits = mmio_read(GICD_BASE, GICD_ISENABLER0);
    mmio_write(GICD_BASE, GICD_ISENABLER0, enable_bits | (1 << 30));

    // Set priority for ID 30 to a default value (e.g., 0xA0)
    // ID 30 is the 3rd byte inside IPRIORITYR7 (byte offset 2)
    let mut priority = mmio_read(GICD_BASE, GICD_IPRIORITYR7);
    priority &= !(0xFF << 16); // Clear old priority
    priority |= 0xA0 << 16; // Set new priority
    mmio_write(GICD_BASE, GICD_IPRIORITYR7, priority);

    // Enable distributor (Group 1 / Non-Secure interrupts)
    mmio_write(GICD_BASE, GICD_CTLR, 1);

    // -- cpu interface setup --
    // TODO: per core

    // Allow all interrupts with priorities higher than 0xF0
    mmio_write(GICC_BASE, GICC_PMR, 0xF0);

    // Enable the CPU interface signaling
    mmio_write(GICC_BASE, GICC_CTLR, 1);
}

pub unsafe fn timer_set_timeout(ms: u64) {
    let freq: u64;
    // Read the system counter frequency (Hz)
    asm!("mrs {}, cntfrq_el0", out(reg) freq);

    let ticks = (freq / 1000) * ms;

    // Write countdown value
    asm!("msr cntp_tval_el0, {}", in(reg) ticks);

    // Enable timer, unmask interrupt (Bit 0 = 1, Bit 1 = 0)
    asm!("msr cntp_ctl_el0, {}", in(reg) 1_u64);
}

pub unsafe fn timer_clear() {
    // Mask the timer interrupt temporarily or turn it off
    // Setting Bit 1 (IMASK) hides the interrupt until a new tval is written
    asm!("msr cntp_ctl_el0, {}", in(reg) 3_u64);
}

pub fn timer_get_absolute_time_ms() -> u64 {
    unsafe {
        let freq: u64;
        // Read the system counter frequency (Hz)
        asm!("mrs {}, cntfrq_el0", out(reg) freq);

        let mut ticks: u64;
        asm!("mrs {}, cntpct_el0", out(reg) ticks);

        (ticks * 1000) / freq
    }
}

pub unsafe fn handle_irq() {
    // Read Interrupt Acknowledge Register
    let iar = mmio_read(GICC_BASE, GICC_IAR);
    let interrupt_id = iar & 0x3FF; // Mask out CPU ID fields (bits 10-12)

    // Route the interrupt based on ID
    match interrupt_id {
        30 => {
            // Non-Secure Physical Timer
            println!("  irq: Timer Ticked!");

            // CRITICAL: The arch timer is level-triggered.
            // We must mask or update the timer hardware *before* updating the GIC.
            // timer_clear();

            // Schedule next event (e.g., 1000ms later)
            timer_set_timeout(1000);
        }
        1023 => {
            // Spurious interrupt indicator; ignore safely.
            return;
        }
        _ => {
            println!("Unhandled interrupt ID: {}", interrupt_id);
        }
    }

    // Signal End of Interrupt (EOI) to the GIC CPU Interface
    // This tells the GIC we are done processing this priority layer.
    mmio_write(GICC_BASE, GICC_EOIR, iar);
}
