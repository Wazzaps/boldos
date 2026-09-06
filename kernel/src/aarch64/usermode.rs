use crate::aarch64::exceptions::ExceptionContext;
use crate::aarch64::mmu;
use crate::aarch64::mmu::{tlb_flush, PageTable};
use crate::drv::arm_gic::{timer_get_absolute_time_ms, timer_set_timeout};
use crate::drv::qemu_console::puts;
use crate::intrusive_rc::IntrusiveRc;
use crate::page_alloc::{add_memory_node, PageBox, PhyAddr, PAGE_ALLOC, PAGE_SIZE};
use crate::{drv, page_alloc, println};
use aarch64_cpu::registers::{ELR_EL1, SPSR_EL1, SP_EL0, TTBR0_EL1};
use core::arch::asm;
use core::marker::PhantomData;
use core::mem::{forget, MaybeUninit};
use core::ops::{Deref, DerefMut};
use kernel_api::kernel_device::KernelDeviceId;
use kernel_api::{
    kernel_device, ControlThreadOp, CreateThreadFlags, KError, MemMapFlags, PhyMapFlags, Pid,
    Syscall,
};
use tock_registers::interfaces::Writeable;
use zerocopy::{FromZeros, IntoBytes};

struct Thread {
    page_table: IntrusiveRc<PageTable>,
    stack: PageBox<[u64; 2048]>, // 16KiB stack
    vals: ExceptionContext,
    sleep_deadline: u64,
    last_scheduled_time: u64,
    state: ThreadState,
    locked: bool,
}

impl Thread {
    pub unsafe fn load(&mut self) {
        TTBR0_EL1.set_baddr(PhyAddr::from_virt(self.page_table.as_ptr()).0 as u64);
        SPSR_EL1.set(self.vals.spsr);
        SP_EL0.set(self.vals.sp);
        ELR_EL1.set(self.vals.pc);
        tlb_flush();
    }

    pub unsafe fn enter(&mut self) -> ! {
        self.load();
        // The guard that holds this lock won't be dropped, unlock it manually
        self.locked = false;
        asm!("eret", options(noreturn))
    }

    pub fn save(&mut self, e: &mut ExceptionContext) {
        self.vals = *e;
    }
}

const DEFAULT_PC: u64 = 0x10000000;
const DEFAULT_SP: u64 = 0x8000000;
const DEFAULT_STACK_SIZE: u64 = 0x4000;

impl Thread {
    fn new() -> Self {
        Self {
            page_table: IntrusiveRc::uninit(),
            stack: PageBox::new_zeroed(),
            vals: ExceptionContext {
                gpr: [0; 30],
                lr: 0,
                pc: DEFAULT_PC,
                sp: DEFAULT_SP,
                spsr: 0x140,
            },
            sleep_deadline: 0,
            last_scheduled_time: 0,
            state: ThreadState::Paused,
            locked: false,
        }
    }
}

#[derive(Debug, PartialEq)]
enum ThreadState {
    Sleeping,
    Runnable,
    Running,
    Paused,
}

static INIT_BIN: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/init.bin"));

static mut THREAD_MANAGER: MaybeUninit<PageBox<ThreadManager>> = MaybeUninit::uninit();
const THREAD_BLOCK_SIZE: usize = 128;
const SCHEDULE_INTERVAL_MS: u64 = 30;

struct ThreadManager {
    block: PageBox<ThreadBlock>,
    thread_bitmap: [u64; THREAD_BLOCK_SIZE / 64],
    current_thread: u32,
}

#[derive(FromZeros)]
struct ThreadBlock {
    threads: [MaybeUninit<Thread>; THREAD_BLOCK_SIZE],
}

impl ThreadBlock {
    /// # Safety
    ///
    /// The given index must be valid and the thread must be initialized
    unsafe fn get_thread_unchecked(&self, idx: usize) -> ThreadLockGuard<'_> {
        let thread = self.threads[idx].assume_init_ref() as *const Thread as *mut Thread;
        ThreadLockGuard::lock(thread)
    }
}

impl ThreadManager {
    unsafe fn init_global() {
        #[allow(static_mut_refs)]
        let mgr = THREAD_MANAGER.write(PageBox::new(ThreadManager {
            block: PageBox::new_zeroed(),
            thread_bitmap: [0; THREAD_BLOCK_SIZE / 64],
            current_thread: 0,
        }));

        // Initialize the init thread
        let (pid, mut thread) = mgr.create_thread(None);
        thread.state = ThreadState::Running;
        thread.last_scheduled_time = timer_get_absolute_time_ms();
        drop(thread);
        mgr.current_thread = pid;
    }

    unsafe fn get_global() -> &'static mut ThreadManager {
        #[allow(static_mut_refs)]
        THREAD_MANAGER.assume_init_mut()
    }

    fn get_current_thread(&self) -> ThreadLockGuard<'_> {
        self.get_thread(self.current_thread)
    }

    fn create_thread(&mut self, share_page_table: Option<Pid>) -> (Pid, ThreadLockGuard<'_>) {
        // TODO: this currently doesn't share the page table with the parent thread
        for pid in 0..THREAD_BLOCK_SIZE {
            let block_idx = pid / 64;
            let bit_idx = pid % 64;
            if self.thread_bitmap[block_idx] & (1 << bit_idx) == 0 {
                println!(" user: Creating thread {}", pid + 1);
                self.block.threads[pid].write(Thread::new());

                let new_thread = unsafe {
                    let mut new_thread = self.block.get_thread_unchecked(pid);

                    if let Some(share_pid) = share_page_table {
                        let mut share_thread = self.get_thread(share_pid);
                        new_thread
                            .page_table
                            .init_pinned_sibling(&mut share_thread.page_table);

                        let stack_base_phys = PhyAddr::from_virt(new_thread.stack.as_ptr());
                        let stack_base_virt = new_thread.page_table.as_mut().vmap(
                            stack_base_phys,
                            DEFAULT_STACK_SIZE as usize,
                            DEFAULT_PAGE_FLAGS,
                        );
                        new_thread.vals.sp = stack_base_virt as u64 + DEFAULT_STACK_SIZE;
                    } else {
                        // SAFETY: The thread will not move until it's dropped
                        new_thread.page_table.init_pinned(PageBox::new_zeroed());

                        for stack_page in (0..DEFAULT_STACK_SIZE).step_by(PAGE_SIZE) {
                            let phy_addr = PhyAddr::from_virt(
                                new_thread.stack.as_ptr().byte_offset(stack_page as isize),
                            );
                            new_thread.page_table.as_mut().vmap_at(
                                (DEFAULT_SP - DEFAULT_STACK_SIZE + stack_page) as usize,
                                phy_addr,
                                DEFAULT_PAGE_FLAGS,
                            );
                        }
                    }

                    new_thread
                };

                self.thread_bitmap[block_idx] |= 1 << bit_idx;
                return ((pid + 1) as u32, new_thread);
            }
        }
        panic!("No free thread slots");
    }

    fn get_thread(&self, pid: Pid) -> ThreadLockGuard<'_> {
        assert!(pid > 0, "Thread 0 is invalid");
        let pid_idx = pid - 1;
        let block_idx = pid_idx / 64;
        let bit_idx = pid_idx % 64;
        if self.thread_bitmap[block_idx as usize] & (1 << bit_idx) == 0 {
            panic!("Thread {pid_idx} not found");
        }
        unsafe { self.block.get_thread_unchecked(pid_idx as usize) }
    }

    fn get_next_deadline(&mut self) -> (Pid, u64) {
        let mut min_deadline = u64::MAX;
        let mut min_sched_time = u64::MAX;
        let mut min_pid = 0;
        for pid in 0..THREAD_BLOCK_SIZE {
            let block_idx = pid / 64;
            let bit_idx = pid % 64;
            if self.thread_bitmap[block_idx as usize] & (1 << bit_idx) == 0 {
                continue;
            }
            let is_current_thread = self.current_thread == (pid + 1) as u32;
            let thread = self.get_thread(pid as Pid + 1);
            if (thread.state == ThreadState::Running && !is_current_thread)
                || thread.state == ThreadState::Paused
            {
                continue;
            }
            if thread.sleep_deadline < min_deadline {
                min_deadline = thread.sleep_deadline;
                min_pid = pid + 1;
            }
            if thread.sleep_deadline == 0 && thread.last_scheduled_time < min_sched_time {
                min_sched_time = thread.last_scheduled_time;
                min_pid = pid + 1;
            }
        }
        (min_pid as Pid, min_deadline)
    }

    unsafe fn schedule(&mut self, e: &mut ExceptionContext) {
        let (next_pid, next_deadline) = loop {
            let (next_pid, next_deadline) = self.get_next_deadline();
            if next_pid != 0 {
                break (next_pid, next_deadline);
            }
            println!(" user: No thread to schedule, waiting for interrupt");
            unsafe { asm!("wfi") }
        };

        if next_pid != self.current_thread {
            // println!(
            //     " user: Switching from thread {} to thread {}",
            //     self.current_thread, next_pid
            // );
            // Save current thread
            let mut current_thread = self.get_current_thread();
            current_thread.save(e);
            if current_thread.state == ThreadState::Running {
                current_thread.state = ThreadState::Runnable;
            }
            drop(current_thread);

            // Switch to next thread
            self.current_thread = next_pid;
            let mut next_thread = self.get_current_thread();
            next_thread.load();
            next_thread.last_scheduled_time = timer_get_absolute_time_ms();
            next_thread.state = ThreadState::Running;
            *e = next_thread.vals;
        }

        loop {
            let sleep_left = next_deadline.saturating_sub(timer_get_absolute_time_ms());
            if sleep_left == 0 {
                self.get_current_thread().sleep_deadline = 0;
                timer_set_timeout(SCHEDULE_INTERVAL_MS);
                break;
            }
            if sleep_left > SCHEDULE_INTERVAL_MS {
                println!(" user: cpu idling for: {sleep_left}ms");
            }
            timer_set_timeout(sleep_left);
            unsafe { asm!("wfi") }
        }
    }
}

struct ThreadLockGuard<'a> {
    thread: *mut Thread,
    _phantom: PhantomData<&'a Thread>,
}

impl<'a> ThreadLockGuard<'a> {
    /// # Safety
    ///
    /// The given thread must be valid
    pub unsafe fn lock(thread: *mut Thread) -> Self {
        // println!(" user: Locking thread {:?}", thread);
        assert!(!(*thread).locked);
        (*thread).locked = true;
        Self {
            thread,
            _phantom: PhantomData,
        }
    }
}

impl<'a> Deref for ThreadLockGuard<'a> {
    type Target = Thread;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.thread }
    }
}

impl<'a> DerefMut for ThreadLockGuard<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.thread }
    }
}

impl<'a> Drop for ThreadLockGuard<'a> {
    fn drop(&mut self) {
        unsafe {
            // println!(" user: Unlocking thread {:?}", self.thread);
            assert!((*self.thread).locked);
            (*self.thread).locked = false;
        }
    }
}

const DEFAULT_PAGE_FLAGS: u64 = mmu::PT_RW_EL0 | // non-privileged
        mmu::PT_ISH | // inner shareable
        mmu::PT_MEM; // normal memory

unsafe fn map_init_binary(thread: &mut Thread) {
    let mut code_slice = PAGE_ALLOC
        .lock()
        .alloc_zeroed(INIT_BIN.len().div_ceil(PAGE_SIZE))
        .expect("OOM");
    code_slice.as_mut_slice()[..INIT_BIN.len()].copy_from_slice(INIT_BIN);

    for code_page in 0..INIT_BIN.len().div_ceil(PAGE_SIZE) {
        thread.page_table.as_mut().vmap_at(
            DEFAULT_PC as usize + code_page * PAGE_SIZE,
            PhyAddr::from_virt(
                code_slice
                    .as_ptr()
                    .byte_offset((code_page * PAGE_SIZE) as isize),
            ),
            DEFAULT_PAGE_FLAGS,
        );
    }
    core::mem::forget(code_slice);
}

pub unsafe fn start() {
    println!(" user: Starting usermode");

    ThreadManager::init_global();
    let mut thread = ThreadManager::get_global().get_current_thread();
    map_init_binary(&mut thread);
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
            let mut thread = ThreadManager::get_global().get_current_thread();

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

            e.gpr[0] = thread.page_table.as_mut().vmap(
                PhyAddr(phy_addr as usize),
                len as usize,
                page_flags,
            ) as u64;
        }
        Syscall::MemMap => {
            let len = e.gpr[0];
            let flags = MemMapFlags::from_bits_truncate(e.gpr[1]);
            let mut thread = ThreadManager::get_global().get_current_thread();

            let mut page_flags: u64 = mmu::PT_ISH | mmu::PT_MEM; // inner shareable
            if flags.contains(MemMapFlags::ReadWrite) {
                page_flags |= mmu::PT_RW_EL0;
            } else {
                page_flags |= mmu::PT_RO_EL0;
            }

            // TODO: support fragmented physical memory
            let page_slice = page_alloc::alloc(len.div_ceil(PAGE_SIZE as u64) as usize);
            let phy_addr = PhyAddr::from_virt(page_slice.as_ptr());
            e.gpr[0] = thread
                .page_table
                .as_mut()
                .vmap(phy_addr, len as usize, page_flags) as u64;
            forget(page_slice); // Don't free the memory we just allocated
        }
        Syscall::MemUnmap => {
            let virt_addr = e.gpr[0];
            let len = e.gpr[1];
            let mut thread = ThreadManager::get_global().get_current_thread();

            thread
                .page_table
                .as_mut()
                .vunmap(virt_addr as usize, len as usize);

            // TODO: if within range of ram, free the corresponding PageSlice

            e.gpr[0] = 0;
        }
        Syscall::DownloadMoreRam => {
            let phy_addr = e.gpr[0];
            let len = e.gpr[1];
            add_memory_node(PhyAddr(phy_addr as usize), len as usize);
        }
        Syscall::LoadKernelDevice => {
            let ptr = e.gpr[0];
            let len = e.gpr[1];
            let dev_id = e.gpr[2];

            if dev_id == kernel_device::GicAndTimer::ID as u64 {
                let mut gic_and_timer = kernel_device::GicAndTimer::new_zeroed();
                assert_eq!(
                    len,
                    size_of_val(&gic_and_timer) as u64,
                    "Invalid length for GicAndTimer"
                );
                copy_from_user(ptr as usize, len as usize, gic_and_timer.as_mut_bytes());
                println!(" user: LoadKernelDevice: {:#x?}", gic_and_timer);

                drv::arm_gic::timer_clear();
                drv::arm_gic::init_gic(
                    gic_and_timer.gicd_base as usize,
                    gic_and_timer.gicc_base as usize,
                    gic_and_timer.timer_ppi_interrupt,
                );

                // Start scheduling
                timer_set_timeout(SCHEDULE_INTERVAL_MS);

                e.gpr[0] = 0;
                return;
            } else {
                e.gpr[0] = KError::InvalidArgument.into();
                return;
            }
        }
        Syscall::SleepSec => {
            let sec = e.gpr[0];
            let deadline: u64 = timer_get_absolute_time_ms() + sec * 1000;
            // println!(" user: Sleeping for {} seconds", sec);
            let mgr = ThreadManager::get_global();
            {
                let mut thread = mgr.get_current_thread();
                thread.sleep_deadline = deadline;
                thread.state = ThreadState::Sleeping;
            }
            mgr.schedule(e);
            {
                let mut thread = mgr.get_current_thread();
                if thread.state == ThreadState::Sleeping {
                    thread.state = ThreadState::Running;
                }
            }
            e.gpr[0] = 0;
        }
        Syscall::CreateThread => {
            let func = e.gpr[0];
            let flags = CreateThreadFlags::from_bits_truncate(e.gpr[1]);

            let mgr = ThreadManager::get_global();
            let share_page_table = if flags.contains(CreateThreadFlags::SharePageTable) {
                Some(mgr.current_thread)
            } else {
                None
            };

            let (pid, mut thread) = mgr.create_thread(share_page_table.clone());
            thread.vals.pc = func;
            if share_page_table.is_none() {
                // TODO: Load executables in user mode
                map_init_binary(&mut thread);
            }
            e.gpr[0] = pid as u64;
            return;
        }
        Syscall::GetPid => {
            e.gpr[0] = ThreadManager::get_global().current_thread as u64;
            return;
        }
        Syscall::ControlThread => {
            let Ok(pid): Result<u32, _> = e.gpr[0].try_into() else {
                println!("Invalid thread ID: {}", e.gpr[0]);
                e.gpr[0] = KError::InvalidArgument.into();
                return;
            };
            let Ok(op): Result<u32, _> = e.gpr[1].try_into() else {
                println!("Invalid control thread operation: {}", e.gpr[1]);
                e.gpr[0] = KError::InvalidArgument.into();
                return;
            };
            let Ok(op) = ControlThreadOp::try_from(op) else {
                println!("Unknown control thread operation: {}", e.gpr[1]);
                e.gpr[0] = KError::InvalidArgument.into();
                return;
            };
            let mgr = ThreadManager::get_global();
            match op {
                ControlThreadOp::Pause => {
                    mgr.get_thread(pid).state = ThreadState::Paused;
                    if pid == mgr.current_thread {
                        todo!("Suspended own thread, need to switch to next thread");
                    }
                    e.gpr[0] = 0;
                    return;
                }
                ControlThreadOp::Resume => {
                    mgr.get_thread(pid).state = ThreadState::Runnable;
                    e.gpr[0] = 0;
                    return;
                }
            }
        }
    }
}

pub unsafe fn handle_timer_tick(e: &mut ExceptionContext) {
    ThreadManager::get_global().schedule(e);
}
