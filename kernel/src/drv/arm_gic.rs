use crate::{page_alloc::PhyAddr, println};
use core::arch::asm;
use core::ptr::{read_volatile, write_volatile};

static mut GICD_BASE: usize = 0;
static mut GICC_BASE: usize = 0;

const GICD_CTLR: usize = 0x000;
const GICD_ISENABLER0: usize = 0x100;
const GICD_IPRIORITYR0: usize = 0x400;

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

pub unsafe fn init_gic(gicd_base: usize, gicc_base: usize, timer_ppi_interrupt: u32) {
    (&raw mut GICD_BASE).write(gicd_base);
    (&raw mut GICC_BASE).write(gicc_base);
    println!("  drv: Initializing ARM GIC");

    // -- distributor setup --

    // Disable distributor during configuration
    mmio_write(gicd_base, GICD_CTLR, 0);

    // Enable the timer PPI
    let reg_idx = timer_ppi_interrupt as usize / 32;
    let interrupt_enable_reg = GICD_ISENABLER0 + (reg_idx * 4);
    let interrupt_enable_shift = timer_ppi_interrupt % 32;
    let enable_bits = mmio_read(gicd_base, interrupt_enable_reg);
    mmio_write(
        gicd_base,
        interrupt_enable_reg,
        enable_bits | (1 << interrupt_enable_shift),
    );

    // Set priority for the timer PPI to a default value (e.g., 0xA0)
    let reg_idx = timer_ppi_interrupt as usize / 4;
    let interrupt_priority_reg = GICD_IPRIORITYR0 + (reg_idx * 4);
    let interrupt_priority_shift = (timer_ppi_interrupt % 4) * 8;
    let mut priority = mmio_read(gicd_base, interrupt_priority_reg);
    priority &= !(0xFF << interrupt_priority_shift); // Clear old priority
    priority |= 0xA0 << interrupt_priority_shift; // Set new priority
    mmio_write(gicd_base, interrupt_priority_reg, priority);

    // Enable distributor (Group 1 / Non-Secure interrupts)
    mmio_write(gicd_base, GICD_CTLR, 1);

    // -- cpu interface setup --
    // TODO: per core

    // Allow all interrupts with priorities higher than 0xF0
    mmio_write(gicc_base, GICC_PMR, 0xF0);

    // Enable the CPU interface signaling
    mmio_write(gicc_base, GICC_CTLR, 1);
}

pub unsafe fn timer_set_timeout(ms: u64) {
    let gicc_base = (&raw const GICC_BASE).read();
    assert_ne!(gicc_base, 0, "GIC must be initialized before sleeping");

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
    let gicc_base = (&raw const GICC_BASE).read();

    // Read Interrupt Acknowledge Register
    let iar = mmio_read(gicc_base, GICC_IAR);
    let interrupt_id = iar & 0x3FF; // Mask out CPU ID fields (bits 10-12)

    // Route the interrupt based on ID
    match interrupt_id {
        30 => {
            // Non-Secure Physical Timer
            println!("  irq: Timer Ticked!");

            // Clear the timer interrupt so it stops triggering
            timer_clear();

            // TODO: Schedule next event
        }
        1023 => {
            // Spurious interrupt
            return;
        }
        _ => {
            println!("Unhandled interrupt ID: {interrupt_id}");
        }
    }

    // Signal End of Interrupt (EOI) to the GIC CPU Interface
    // This tells the GIC we are done processing this priority layer.
    mmio_write(gicc_base, GICC_EOIR, iar);
}
