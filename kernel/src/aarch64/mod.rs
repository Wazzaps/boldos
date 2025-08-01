use core::arch::global_asm;

mod exceptions;
pub mod interrupts;
pub mod mmu;
pub mod usermode;

global_asm!(include_str!("init.s"));

#[macro_export]
macro_rules! set_msr_const {
    ($name: ident, $value: expr) => {
        ::core::arch::asm!(
            concat!("msr ", stringify!($name), ", {}"),
            const $value,
            options(nomem, nostack)
        )
    };
}

#[macro_export]
macro_rules! get_msr {
    ($name: ident) => {{
        let val: u64;
        ::core::arch::asm!(
            concat!("mrs {:x}, ", stringify!($name)),
            out(reg) val,
            options(nomem, nostack)
        );
        val
    }};
}
