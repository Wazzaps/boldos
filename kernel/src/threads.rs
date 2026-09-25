use crate::aarch64::exceptions::ExceptionContext;
use crate::aarch64::mmu;
use crate::aarch64::mmu::{tlb_flush, PageTable};
use crate::drv::arm_gic::{timer_get_absolute_time_ms, timer_set_timeout};
use crate::intrusive_rc::IntrusiveRc;
use crate::ipc::{Futex, Handle, HandleDataRef, Port};
use crate::page_alloc::{self, PageBox, PhyAddr, PAGE_ALLOC, PAGE_SIZE};
use crate::println;
use aarch64_cpu::registers::{ELR_EL1, SPSR_EL1, SP_EL0, TTBR0_EL1};
use core::arch::asm;
use core::cell::{Cell, UnsafeCell};
use core::marker::PhantomData;
use core::mem::{forget, MaybeUninit};
use core::ops::{Deref, DerefMut};
use core::ptr::null_mut;
use kernel_api::{KError, MemMapFlags, PhyMapFlags, Pid};
use tock_registers::interfaces::Writeable;
use zerocopy::FromZeros;

const THREAD_BLOCK_SIZE: usize = 128;
const FUTEX_BLOCK_SIZE: usize = 256;
const HANDLE_BLOCK_SIZE: usize = 256;
const PORT_BLOCK_SIZE: usize = 256;
const SCHEDULE_INTERVAL_MS: u64 = 30;

const DEFAULT_PC: usize = 0x10000000;
const DEFAULT_SP: usize = 0x8000000;
const DEFAULT_STACK_SIZE: usize = 16 * 1024;

const DEFAULT_PAGE_FLAGS: u64 = mmu::PT_RW_EL0 | // non-privileged
        mmu::PT_ISH | // inner shareable
        mmu::PT_MEM; // normal memory

static INIT_BIN: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/init.bin"));

static mut THREAD_MANAGER: MaybeUninit<PageBox<ThreadManager>> = MaybeUninit::uninit();

pub struct Thread {
    page_table: IntrusiveRc<PageTable>,
    stack: PageBox<[u64; DEFAULT_STACK_SIZE / 8]>,
    pub regs: ExceptionContext,
    sleep_deadline: u64,
    last_scheduled_time: u64,
    state: ThreadState,
    locked: bool,
    handles: PageBox<Block<Handle, HANDLE_BLOCK_SIZE>>,
}

impl Thread {
    fn new() -> Self {
        Self {
            page_table: IntrusiveRc::uninit(),
            stack: PageBox::new_zeroed(),
            regs: ExceptionContext {
                gpr: [0; 30],
                lr: 0,
                pc: DEFAULT_PC as u64,
                sp: DEFAULT_SP as u64,
                spsr: 0x140,
            },
            sleep_deadline: 0,
            last_scheduled_time: 0,
            state: ThreadState::Paused,
            locked: false,
            handles: PageBox::new_zeroed(),
        }
    }

    pub unsafe fn load(&mut self) {
        TTBR0_EL1.set_baddr(PhyAddr::from_virt(self.page_table.as_ptr()).0 as u64);
        SPSR_EL1.set(self.regs.spsr);
        SP_EL0.set(self.regs.sp);
        ELR_EL1.set(self.regs.pc);
        tlb_flush();
    }

    pub unsafe fn enter(&mut self) -> ! {
        self.load();
        // The guard that holds this lock won't be dropped, unlock it manually
        assert!(self.locked);
        self.locked = false;
        // println!(" user: Entering thread {:?}", &raw const self);
        asm!("eret", options(noreturn))
    }

    pub fn save(&mut self, e: &mut ExceptionContext) {
        self.regs = *e;
    }

    pub unsafe fn add_handle(&self, id: u64, flags: u16, data: HandleDataRef<'_>) {
        for i in 0..HANDLE_BLOCK_SIZE {
            let handle = self.handles.items[i].get();
            if (*handle).id == 0 {
                (*handle).id = id;
                (*handle).flags = flags;
                (*handle).set_data(data);
                return;
            }
        }
        panic!("Handle block full");
    }

    pub unsafe fn remove_handle(&self, id: u64, mgr: &ThreadManager) {
        for i in 0..HANDLE_BLOCK_SIZE {
            let handle = self.handles.items[i].get();
            if (*handle).id == id {
                (*handle).id = 0;
                match (*handle).data() {
                    HandleDataRef::Port(port) => {
                        mgr.free_port(port.port);
                    }
                    _ => todo!("Removing other handle types not implemented"),
                }
                (*handle).flags = 0;
                (*handle).handle_type = 0;
                return;
            }
        }
        panic!("Handle not found");
    }

    pub unsafe fn find_handle(&self, id: u64) -> *mut Handle {
        for i in 0..HANDLE_BLOCK_SIZE {
            let handle = self.handles.items[i].get();
            if (*handle).id == id {
                return handle;
            }
        }
        return null_mut();
    }

    pub fn start_sleep(&mut self, deadline_ms: u64) {
        self.sleep_deadline = deadline_ms;
        self.state = ThreadState::Sleeping;
    }

    pub fn start_futex_wait(&mut self, timeout_micros: u64) {
        self.state = ThreadState::FutexWaiting;
        if timeout_micros == u64::MAX {
            self.sleep_deadline = 0;
        } else {
            self.sleep_deadline =
                timer_get_absolute_time_ms() + timeout_micros.div_ceil(1000).max(1);
        }
    }

    pub unsafe fn map_init_binary(&mut self) {
        let mut code_slice = PAGE_ALLOC
            .lock()
            .alloc_zeroed(INIT_BIN.len().div_ceil(PAGE_SIZE))
            .expect("OOM");
        code_slice.as_mut_slice()[..INIT_BIN.len()].copy_from_slice(INIT_BIN);

        for code_page in 0..INIT_BIN.len().div_ceil(PAGE_SIZE) {
            self.page_table.as_mut().vmap_at(
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

    pub fn pause(&mut self) {
        // FIXME: this discards the current state
        self.state = ThreadState::Paused;
    }

    pub fn resume(&mut self) {
        // FIXME: this mistakenly wakes up threads that were sleeping
        self.state = ThreadState::Runnable;
    }

    pub unsafe fn mem_map(&mut self, len: usize, flags: MemMapFlags) -> usize {
        let mut page_flags: u64 = mmu::PT_ISH | mmu::PT_MEM; // inner shareable
        if flags.contains(MemMapFlags::ReadWrite) {
            page_flags |= mmu::PT_RW_EL0;
        } else {
            page_flags |= mmu::PT_RO_EL0;
        }

        // TODO: support fragmented physical memory
        let page_slice = page_alloc::alloc_zeroed(len.div_ceil(PAGE_SIZE));
        let phy_addr = PhyAddr::from_virt(page_slice.as_ptr());
        forget(page_slice); // Don't free the memory we just allocated
        self.page_table
            .as_mut()
            .vmap(phy_addr, len as usize, page_flags)
    }

    pub unsafe fn mem_unmap(&mut self, virt_addr: usize, len: usize) {
        self.page_table.as_mut().vunmap(virt_addr, len);

        // TODO: if within range of ram, free the corresponding PageSlice
    }

    pub fn virt_to_phys(&self, virt_addr: usize) -> Option<PhyAddr> {
        self.page_table.as_ref().virt_to_phys(virt_addr)
    }
}

#[derive(Debug, PartialEq)]
enum ThreadState {
    Sleeping,
    FutexWaiting,
    Runnable,
    Running,
    Paused,
}

pub struct ThreadManager {
    block: PageBox<Block<MaybeUninit<Thread>, THREAD_BLOCK_SIZE>>,
    thread_bitmap: [u64; THREAD_BLOCK_SIZE / 64],
    pub current_thread: u32,

    // TODO: make private, use proper locks
    pub futexes_locked: Cell<bool>,
    pub active_futexes: PageBox<Block<Futex, FUTEX_BLOCK_SIZE>>,

    // TODO: make private, use proper locks
    pub ipc_locked: Cell<bool>,
    pub ports: PageBox<Block<Port, PORT_BLOCK_SIZE>>,
    pub next_handle_id: u64,

    pub last_log_was_newline: bool,
}

impl ThreadManager {
    unsafe fn init_global() {
        #[allow(static_mut_refs)]
        let mgr = THREAD_MANAGER.write(PageBox::new(ThreadManager {
            block: PageBox::new_zeroed(),
            thread_bitmap: [0; THREAD_BLOCK_SIZE / 64],
            current_thread: 0,

            futexes_locked: Cell::new(false),
            active_futexes: PageBox::new_zeroed(),

            ipc_locked: Cell::new(false),
            ports: PageBox::new_zeroed(),
            next_handle_id: 1,

            last_log_was_newline: true,
        }));

        // Initialize the init thread
        let (pid, mut thread) = mgr.create_thread(None);
        thread.state = ThreadState::Running;
        thread.last_scheduled_time = timer_get_absolute_time_ms();
        drop(thread);
        mgr.current_thread = pid;
    }

    pub unsafe fn get_global() -> &'static mut ThreadManager {
        #[allow(static_mut_refs)]
        THREAD_MANAGER.assume_init_mut()
    }

    fn next_pid(&self, pid: Pid) -> Pid {
        assert!(pid > 0, "Thread 0 is invalid");
        let pid_idx = pid - 1;
        // Start from the next thread
        for idx in (pid_idx as usize + 1)..THREAD_BLOCK_SIZE {
            let block_idx = idx / 64;
            let bit_idx = idx % 64;
            if self.thread_bitmap[block_idx] & (1 << bit_idx) != 0 {
                return (idx + 1) as Pid;
            }
        }
        return 0;
    }

    pub fn get_current_thread(&self) -> ThreadLockGuard<'_> {
        self.get_thread(self.current_thread)
    }

    pub fn create_thread(&mut self, share_page_table: Option<Pid>) -> (Pid, ThreadLockGuard<'_>) {
        // TODO: this currently doesn't share the page table with the parent thread
        for pid in 0..THREAD_BLOCK_SIZE {
            let block_idx = pid / 64;
            let bit_idx = pid % 64;
            if self.thread_bitmap[block_idx] & (1 << bit_idx) == 0 {
                println!(" user: Creating thread {}", pid + 1);

                let new_thread = unsafe {
                    let mut new_thread = self.block.new_thread(pid);

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
                        new_thread.regs.sp = stack_base_virt as u64 + DEFAULT_STACK_SIZE as u64;
                    } else {
                        // SAFETY: The thread will not move until it's dropped
                        new_thread.page_table.init_pinned(PageBox::new_zeroed());

                        for stack_page in (0..DEFAULT_STACK_SIZE).step_by(PAGE_SIZE) {
                            let phy_addr = PhyAddr::from_virt(
                                new_thread.stack.as_ptr().byte_offset(stack_page as isize),
                            );
                            new_thread.page_table.as_mut().vmap_at(
                                DEFAULT_SP - DEFAULT_STACK_SIZE + stack_page,
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

    pub fn get_thread(&self, pid: Pid) -> ThreadLockGuard<'_> {
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
                || (thread.state == ThreadState::FutexWaiting && thread.sleep_deadline == 0)
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

    pub unsafe fn schedule(&mut self, e: &mut ExceptionContext) {
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
            *e = next_thread.regs;
        }

        loop {
            let sleep_left = next_deadline.saturating_sub(timer_get_absolute_time_ms());
            if sleep_left == 0 {
                let mut current_thread = self.get_current_thread();
                if current_thread.state == ThreadState::FutexWaiting {
                    self.remove_futex_waiter(self.current_thread);
                    if current_thread.sleep_deadline != 0 {
                        println!("erasing 1");
                        e.gpr[0] = KError::TryAgain.into();
                    } else {
                        println!("erasing 2");
                        e.gpr[0] = 0;
                    }
                }
                current_thread.state = ThreadState::Running;
                current_thread.sleep_deadline = 0;
                timer_set_timeout(SCHEDULE_INTERVAL_MS);
                break;
            }
            if sleep_left > SCHEDULE_INTERVAL_MS {
                println!(" user: cpu idling for: {sleep_left}ms");
            }
            timer_set_timeout(sleep_left);
            unsafe { asm!("wfi") }
        }

        assert_ne!(
            self.get_current_thread().state,
            ThreadState::Sleeping,
            "Active thread is sleeping at the end of schedule"
        );
    }

    pub unsafe fn start_schedule_timer(&self) {
        timer_set_timeout(SCHEDULE_INTERVAL_MS);
    }

    pub unsafe fn add_futex_waiter(&self, phy_addr: PhyAddr, pid: Pid, value: u32) {
        for i in 0..FUTEX_BLOCK_SIZE {
            let futex = self.active_futexes.items[i].get();
            if (*futex).phys_addr.0 == 0 {
                (*futex).phys_addr = phy_addr;
                (*futex).pid = pid;
                (*futex).value = value;
                return;
            }
        }
        panic!("Futex block full");
    }

    pub unsafe fn remove_futex_waiter(&self, pid: Pid) {
        for i in 0..FUTEX_BLOCK_SIZE {
            let futex = self.active_futexes.items[i].get().as_mut_unchecked();
            if futex.pid == pid {
                futex.phys_addr = PhyAddr(0);
                futex.pid = 0;
                futex.value = 0;
            }
        }
    }

    pub unsafe fn wake_futex_waiters(
        &self,
        phy_addr: PhyAddr,
        word_value: u32,
        mut wake_count: u32,
    ) {
        for i in 0..FUTEX_BLOCK_SIZE {
            if wake_count == 0 {
                return;
            }
            let futex = self.active_futexes.items[i].get().as_mut_unchecked();
            if futex.phys_addr.0 == phy_addr.0 && futex.value != word_value {
                wake_count -= 1;
                let mut thread = self.get_thread(futex.pid);
                assert_eq!(thread.state, ThreadState::FutexWaiting);
                thread.state = ThreadState::Runnable;
                thread.sleep_deadline = 0;
                futex.phys_addr = PhyAddr(0);
                futex.pid = 0;
                futex.value = 0;
            }
        }
    }

    pub unsafe fn alloc_port(&self, pid: Pid, recv_handle: u64) -> *mut Port {
        for i in 0..PORT_BLOCK_SIZE {
            let port = self.ports.items[i].get();
            if (*port).recv_pid == 0 {
                println!(
                    " user: Allocating port for thread {} at address {:p} with recv_handle {}",
                    pid, port, recv_handle
                );
                (*port).recv_pid = pid;
                (*port).recv_handle = recv_handle;
                // Port creation makes both recv and send handles
                (*port).ref_count = 2;
                return port;
            }
        }
        panic!("Port block full");
    }

    unsafe fn free_port(&self, port: *mut Port) {
        assert!((*port).ref_count > 0);
        (*port).ref_count -= 1;
        if (*port).ref_count == 0 {
            println!(
                " user: Freeing port of thread {} at address {:p}",
                (*port).recv_pid,
                port
            );
            (*port).recv_pid = 0;
            (*port).recv_handle = 0;
        }
    }

    pub unsafe fn thread_phy_map(
        &self,
        pid: Pid,
        phy_addr: u64,
        len: usize,
        flags: PhyMapFlags,
    ) -> PhyMapResult {
        let mut thread = self.get_thread(pid);

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

        if phy_addr == u64::MAX {
            // We get to pick the address
            let page_slice = page_alloc::alloc_zeroed(len.div_ceil(PAGE_SIZE));
            let phy_addr = PhyAddr::from_virt(page_slice.as_ptr());
            forget(page_slice); // Don't free the memory we just allocated
            PhyMapResult {
                virt_addr: thread
                    .page_table
                    .as_mut()
                    .vmap(phy_addr, len as usize, page_flags) as u64,
                phy_addr: phy_addr.0 as u64,
            }
        } else {
            // Must map a specific physical address
            PhyMapResult {
                virt_addr: thread.page_table.as_mut().vmap(
                    PhyAddr(phy_addr as usize),
                    len,
                    page_flags,
                ) as u64,
                phy_addr: phy_addr as u64,
            }
        }
    }
}

impl Block<MaybeUninit<Thread>, THREAD_BLOCK_SIZE> {
    /// # Safety
    ///
    /// The given index must be valid and the thread must be initialized
    unsafe fn get_thread_unchecked(&self, idx: usize) -> ThreadLockGuard<'_> {
        let thread = self.items[idx].get().as_mut_unchecked().as_mut_ptr();
        ThreadLockGuard::lock(thread)
    }

    unsafe fn new_thread(&self, idx: usize) -> ThreadLockGuard<'_> {
        let thread = self.items[idx]
            .get()
            .as_mut_unchecked()
            .write(Thread::new()) as *mut _;
        ThreadLockGuard::lock(thread)
    }
}

pub struct ThreadLockGuard<'a> {
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

pub struct PhyMapResult {
    pub virt_addr: u64,
    pub phy_addr: u64,
}

#[derive(FromZeros)]
pub struct Block<T, const N: usize> {
    items: [UnsafeCell<T>; N],
}

pub unsafe fn handle_timer_tick(e: &mut ExceptionContext) {
    ThreadManager::get_global().schedule(e);
}

pub unsafe fn start() {
    println!(" user: Starting usermode");

    ThreadManager::init_global();
    let mut thread = ThreadManager::get_global().get_current_thread();
    thread.map_init_binary();
    thread.enter();
}
