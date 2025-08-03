use crate::aarch64::exceptions::ExceptionContext;
use crate::aarch64::mmu;
use crate::aarch64::mmu::{tlb_flush, PageTable};
use crate::drv::qemu_console::puts;
use crate::page_alloc::{PageBox, PhyAddr, PAGE_ALLOC, PAGE_SIZE};
use crate::println;
use aarch64_cpu::registers::{ELR_EL1, SPSR_EL1, SP_EL0, TTBR0_EL1};
use core::arch::asm;
use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use kernel_api::{PhyMapFlags, Syscall};
use tock_registers::interfaces::Writeable;
use zerocopy::FromZeros;

#[derive(FromZeros)]
struct Thread {
    /// Virtual memory mapping
    page_table: PageTable,
    /// Stack contents
    stack: [u64; 1024], // 8KiB stack
    /// General-purpose registers
    gprs: [u64; 31],
    /// Link register
    lr: u64,
    /// Program counter
    pc: u64,
    /// Stack pointer
    sp: u64,
    /// Saved program status register
    spsr: u64,
}

impl Thread {
    pub unsafe fn enter(&mut self) -> ! {
        TTBR0_EL1.set_baddr(PhyAddr::from_virt(&raw const self.page_table).0 as u64);
        SPSR_EL1.set(self.spsr);
        SP_EL0.set(self.sp);
        ELR_EL1.set(self.pc);
        tlb_flush();
        asm!("eret", options(noreturn))
    }
}

const DEFAULT_PC: u64 = 0x10000000;
const DEFAULT_SP: u64 = 0x8000000;
const DEFAULT_STACK_SIZE: u64 = 0x4000;

impl Thread {
    fn init(&mut self) {
        self.zero();
        self.pc = DEFAULT_PC;
        self.sp = DEFAULT_SP;
        self.spsr = 0x140;
    }
}

struct GlobalThread(UnsafeCell<MaybeUninit<PageBox<Thread>>>);

impl GlobalThread {
    const unsafe fn uninit() -> Self {
        Self(UnsafeCell::new(MaybeUninit::uninit()))
    }

    unsafe fn init(&self) {
        let mut thread = PageBox::<Thread>::new_zeroed();
        thread.init();
        self.0.get().write(MaybeUninit::new(thread));
    }

    #[allow(dead_code)]
    unsafe fn as_ref(&self) -> &Thread {
        let inner = &*self.0.get();
        inner.assume_init_ref()
    }

    unsafe fn as_mut(&self) -> &mut Thread {
        let inner = &mut *self.0.get();
        inner.assume_init_mut()
    }
}

unsafe impl Sync for GlobalThread {}

static INIT_BIN: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/init.bin"));
static INIT_THREAD: GlobalThread = unsafe { GlobalThread::uninit() };

pub unsafe fn start() {
    println!(" user: Starting usermode");

    let mut code_slice = PAGE_ALLOC
        .lock()
        .alloc_zeroed(INIT_BIN.len().div_ceil(PAGE_SIZE))
        .expect("OOM");
    code_slice.as_mut_slice()[..INIT_BIN.len()].copy_from_slice(INIT_BIN);

    INIT_THREAD.init();
    let thread = INIT_THREAD.as_mut();

    const PAGE_FLAGS: u64 = mmu::PT_RW_EL0 | // non-privileged
        mmu::PT_ISH | // inner shareable
        mmu::PT_MEM; // normal memory
    for code_page in 0..INIT_BIN.len().div_ceil(PAGE_SIZE) {
        thread.page_table.vmap_at(
            DEFAULT_PC as usize + code_page * PAGE_SIZE,
            PhyAddr::from_virt(
                code_slice
                    .as_ptr()
                    .byte_offset((code_page * PAGE_SIZE) as isize),
            ),
            PAGE_FLAGS,
        );
    }
    for stack_page in (0..DEFAULT_STACK_SIZE).step_by(PAGE_SIZE) {
        let phy_addr = PhyAddr::from_virt(thread.stack.as_ptr().byte_offset(stack_page as isize));
        thread.page_table.vmap_at(
            (DEFAULT_SP - DEFAULT_STACK_SIZE + stack_page) as usize,
            phy_addr,
            PAGE_FLAGS,
        );
    }

    thread.enter();
}

unsafe fn copy_from_user(user_pointer: usize, user_len: usize, target: &mut [u8]) {
    assert_eq!(user_len, target.len());
    for i in 0..user_len {
        let mut value: u32;
        // TODO: Use ldtr when possible for performance
        asm!("ldtrb {0:w}, [{1}]", out(reg) value, in(reg) user_pointer + i);
        target[i] = value as u8;
    }
}

pub unsafe fn handle_syscall(e: &mut ExceptionContext) {
    let Ok(syscall_num) = Syscall::try_from(e.gpr[8] as u32) else {
        println!("Unknown syscall: {}", e.gpr[8]);
        e.gpr[0] = u64::MAX;
        return;
    };
    match syscall_num {
        Syscall::Exit => {
            todo!("Syscall::Exit not implemented")
        }
        Syscall::Log => {
            let mut buf = [0u8; 256];
            let ptr = e.gpr[0];
            let len = e.gpr[1].min(buf.len() as u64);
            copy_from_user(ptr as usize, len as usize, &mut buf[..len as usize]);
            puts(&buf[..len as usize]);
        }
        Syscall::PhyMap => {
            let phy_addr = e.gpr[0];
            let len = e.gpr[1];
            let flags = PhyMapFlags::from_bits_truncate(e.gpr[2]);
            let thread = INIT_THREAD.as_mut();

            let mut page_flags: u64 = mmu::PT_ISH; // inner shareable

            if flags.contains(PhyMapFlags::ReadWrite) {
                page_flags |= mmu::PT_RW_EL0;
            } else {
                page_flags |= mmu::PT_RO_EL0;
            }
            if flags.contains(PhyMapFlags::DeviceMem) {
                page_flags |= mmu::PT_DEV;
            } else {
                page_flags |= mmu::PT_MEM;
            }

            e.gpr[0] = thread
                .page_table
                .vmap(PhyAddr(phy_addr as usize), len as usize, page_flags)
                as u64;
        }
        Syscall::VirtUnmap => {
            let virt_addr = e.gpr[0];
            let len = e.gpr[1];
            let thread = INIT_THREAD.as_mut();

            thread.page_table.vunmap(virt_addr as usize, len as usize);

            e.gpr[0] = 0;
        }
    }
}
