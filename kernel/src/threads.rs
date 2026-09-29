use crate::aarch64::exceptions::ExceptionContext;
use crate::aarch64::mmu;
use crate::aarch64::mmu::tlb_flush;
use crate::drv::arm_gic::{timer_get_absolute_time_ms, timer_set_timeout};
use crate::ipc::{Futex, Handle, HandleData, Mm, MmRegion, Port, Region};
use crate::page_alloc::{self, PageBox, PageSlice, PhyAddr, PAGE_ALLOC, PAGE_SIZE};
use crate::println;
use aarch64_cpu::registers::{ELR_EL1, SPSR_EL1, SP_EL0, TTBR0_EL1};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::arch::asm;
use core::cell::{RefCell, RefMut};
use core::mem::{forget, MaybeUninit};
use core::ptr::null_mut;
use kernel_api::{KError, MemMapFlags, PhyMapFlags, Pid};
use tock_registers::interfaces::Writeable;

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
    mm: Arc<RefCell<Mm>>,
    stack: PageBox<[u64; DEFAULT_STACK_SIZE / 8]>,
    pub regs: ExceptionContext,
    sleep_deadline: u64,
    last_scheduled_time: u64,
    state: ThreadState,
    handles: Arc<RefCell<Vec<Handle>>>,
}

impl Thread {
    fn new(
        mm: Arc<RefCell<Mm>>,
        handles: Arc<RefCell<Vec<Handle>>>,
        stack: PageBox<[u64; DEFAULT_STACK_SIZE / 8]>,
        sp: usize,
    ) -> Self {
        Self {
            mm,
            stack,
            regs: ExceptionContext {
                gpr: [0; 30],
                lr: 0,
                pc: DEFAULT_PC as u64,
                sp: sp as u64,
                spsr: 0x140,
            },
            sleep_deadline: 0,
            last_scheduled_time: 0,
            state: ThreadState::Paused,
            handles,
        }
    }

    pub unsafe fn load(&mut self) {
        TTBR0_EL1.set_baddr(self.mm.borrow().page_table.as_phy_addr().0 as u64);
        SPSR_EL1.set(self.regs.spsr);
        SP_EL0.set(self.regs.sp);
        ELR_EL1.set(self.regs.pc);
        tlb_flush();
    }

    pub unsafe fn enter(mut this: RefMut<'_, Self>) -> ! {
        this.load();
        println!(" user: Entering thread {:?}", &raw const *this);
        // Ensure the borrow is dropped before abandoning this stack frame
        drop(this);
        asm!("eret", options(noreturn))
    }

    pub fn save(&mut self, e: &mut ExceptionContext) {
        self.regs = *e;
    }

    pub unsafe fn add_handle(&mut self, id: u64, flags: u16, data: HandleData) {
        self.handles.borrow_mut().push(Handle { id, flags, data });
    }

    pub unsafe fn remove_handle(&mut self, id: u64) {
        let mut handles = self.handles.borrow_mut();
        if let Some(index) = handles.iter().position(|h| h.id == id) {
            handles.swap_remove(index);
        } else {
            panic!("Handle not found");
        }
    }

    // TODO: don't use a pointer
    pub unsafe fn find_handle(&mut self, id: u64) -> *mut Handle {
        if let Some(handle) = self.handles.borrow_mut().iter_mut().find(|h| h.id == id) {
            return handle as *mut Handle;
        }
        return null_mut();
    }

    pub unsafe fn add_mm_region(&mut self, mm_region: MmRegion) {
        self.mm.borrow_mut().regions.push(mm_region);
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
            self.mm.borrow_mut().page_table.vmap_at(
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
        self.mm
            .borrow_mut()
            .page_table
            .as_mut()
            .vmap(phy_addr, len as usize, page_flags)
    }

    pub unsafe fn mem_unmap(&mut self, virt_addr: usize, len: usize) {
        self.mm
            .borrow_mut()
            .page_table
            .as_mut()
            .vunmap(virt_addr, len);

        // TODO: if within range of ram, free the corresponding PageSlice
    }

    pub unsafe fn vmap(&mut self, phy_addr: PhyAddr, len: usize, flags: u64) -> usize {
        self.mm
            .borrow_mut()
            .page_table
            .as_mut()
            .vmap(phy_addr, len, flags)
    }

    pub unsafe fn vunmap(&mut self, virt_addr: usize, len: usize) {
        self.mm
            .borrow_mut()
            .page_table
            .as_mut()
            .vunmap(virt_addr, len);
    }

    pub fn virt_to_phys(&self, virt_addr: usize) -> Option<PhyAddr> {
        self.mm.borrow().page_table.as_ref().virt_to_phys(virt_addr)
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
    threads: Vec<RefCell<MaybeUninit<Thread>>>,
    thread_bitmap: Vec<u64>,
    pub current_thread: Pid,

    active_futexes: RefCell<Vec<Futex>>,

    next_handle_id: u64,

    pub last_log_was_newline: bool,
}

impl ThreadManager {
    unsafe fn init_global() {
        #[allow(static_mut_refs)]
        let mgr = THREAD_MANAGER.write(PageBox::new(ThreadManager {
            threads: Vec::new(),
            thread_bitmap: Vec::new(),
            current_thread: 0,

            active_futexes: RefCell::new(Vec::new()),

            next_handle_id: 1,

            last_log_was_newline: true,
        }));

        // Initialize the init thread
        let (pid, mut thread) = mgr.create_thread(None, None);
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
        for idx in (pid_idx as usize + 1).. {
            let block_idx = idx / 64;
            let bit_idx = idx % 64;
            if block_idx >= self.thread_bitmap.len() {
                break;
            }
            if self.thread_bitmap[block_idx] & (1 << bit_idx) != 0 {
                return (idx + 1) as Pid;
            }
        }
        return 0;
    }

    pub fn get_current_thread(&self) -> ThreadMutLockGuard<'_> {
        self.get_thread_mut(self.current_thread)
    }

    pub fn create_thread(
        &mut self,
        share_page_table: Option<Pid>,
        share_handles: Option<Pid>,
    ) -> (Pid, ThreadMutLockGuard<'_>) {
        for pid_idx in 0.. {
            let block_idx = pid_idx / 64;
            let bit_idx = pid_idx % 64;
            let pid = (pid_idx + 1) as Pid;
            if block_idx >= self.thread_bitmap.len() {
                self.thread_bitmap.push(0);
            }
            if self.thread_bitmap[block_idx] & (1 << bit_idx) == 0 {
                println!(" user: Creating thread {}", pid);

                let new_thread = unsafe {
                    let stack: PageBox<[u64; DEFAULT_STACK_SIZE / 8]> = PageBox::new_zeroed();

                    let (mm, sp) = if let Some(share_pid) = share_page_table {
                        let share_thread = self.get_thread_mut(share_pid);
                        let mm = Arc::clone(&share_thread.mm);

                        let stack_base_phys = PhyAddr::from_virt(stack.as_ptr());
                        let stack_base_virt = mm.borrow_mut().page_table.as_mut().vmap(
                            stack_base_phys,
                            DEFAULT_STACK_SIZE as usize,
                            DEFAULT_PAGE_FLAGS,
                        );
                        let sp = stack_base_virt + DEFAULT_STACK_SIZE;
                        (mm, sp)
                    } else {
                        let mut mm = Mm::new();
                        for stack_page in (0..DEFAULT_STACK_SIZE).step_by(PAGE_SIZE) {
                            let phy_addr =
                                PhyAddr::from_virt(stack.as_ptr().byte_offset(stack_page as isize));
                            mm.page_table.as_mut().vmap_at(
                                DEFAULT_SP - DEFAULT_STACK_SIZE + stack_page,
                                phy_addr,
                                DEFAULT_PAGE_FLAGS,
                            );
                        }
                        (Arc::new(RefCell::new(mm)), DEFAULT_SP)
                    };

                    let handles = if let Some(share_pid) = share_handles {
                        let share_thread = self.get_thread_mut(share_pid);
                        Arc::clone(&share_thread.handles)
                    } else {
                        Arc::new(RefCell::new(Vec::new()))
                    };

                    Thread::new(mm, handles, stack, sp)
                };

                self.thread_bitmap[block_idx] |= 1 << bit_idx;
                if self.threads.len() == pid_idx {
                    self.threads
                        .push(RefCell::new(MaybeUninit::new(new_thread)));
                } else {
                    self.threads[pid_idx as usize]
                        .borrow_mut()
                        .write(new_thread);
                }
                let new_thread = self.threads[pid_idx as usize].borrow_mut();
                let new_thread = unsafe { RefMut::map(new_thread, |t| t.assume_init_mut()) };
                return (pid, new_thread);
            }
        }
        panic!("No free thread slots");
    }

    pub fn get_thread_mut(&self, pid: Pid) -> ThreadMutLockGuard<'_> {
        assert!(pid > 0, "Thread 0 is invalid");
        let pid_idx = pid - 1;
        let block_idx = pid_idx / 64;
        let bit_idx = pid_idx % 64;
        if block_idx as usize >= self.thread_bitmap.len() {
            panic!("Thread {pid_idx} not found");
        }
        if self.thread_bitmap[block_idx as usize] & (1 << bit_idx) == 0 {
            panic!("Thread {pid_idx} not found");
        }
        let thread = self.threads[pid_idx as usize].borrow_mut();
        unsafe { RefMut::map(thread, |t| t.assume_init_mut()) }
    }

    fn get_next_deadline(&mut self) -> (Pid, u64) {
        let mut min_deadline = u64::MAX;
        let mut min_sched_time = u64::MAX;
        let mut min_pid: Pid = 0;
        for pid_idx in 0.. {
            let pid = (pid_idx + 1) as Pid;
            let block_idx = pid_idx / 64;
            let bit_idx = pid_idx % 64;
            if block_idx as usize >= self.thread_bitmap.len() {
                break;
            }
            if self.thread_bitmap[block_idx as usize] & (1 << bit_idx) == 0 {
                continue;
            }
            let is_current_thread = self.current_thread == pid;
            let thread = self.get_thread_mut(pid);
            if (thread.state == ThreadState::Running && !is_current_thread)
                || thread.state == ThreadState::Paused
                || (thread.state == ThreadState::FutexWaiting && thread.sleep_deadline == 0)
            {
                continue;
            }
            if thread.sleep_deadline < min_deadline {
                min_deadline = thread.sleep_deadline;
                min_pid = pid;
            }
            if thread.sleep_deadline == 0 && thread.last_scheduled_time < min_sched_time {
                min_sched_time = thread.last_scheduled_time;
                min_pid = pid;
            }
        }
        (min_pid, min_deadline)
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
                        e.gpr[0] = KError::TryAgain.into();
                    } else {
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

    // TODO: all these futex function may want to hold the futex list locked between them
    pub unsafe fn add_futex_waiter(&self, phy_addr: PhyAddr, pid: Pid, value: u32) {
        self.active_futexes.borrow_mut().push(Futex {
            phy_addr,
            pid,
            value,
        });
    }

    pub unsafe fn remove_futex_waiter(&self, pid: Pid) {
        let mut active_futexes = self.active_futexes.borrow_mut();
        if let Some(index) = active_futexes.iter().position(|f| f.pid == pid) {
            active_futexes.swap_remove(index);
        }
    }

    pub unsafe fn wake_futex_waiters(
        &self,
        phy_addr: PhyAddr,
        word_value: u32,
        mut wake_count: u32,
    ) {
        let mut active_futexes = self.active_futexes.borrow_mut();
        let mut i = 0;
        while i < active_futexes.len() {
            let futex = &active_futexes[i];
            if wake_count == 0 {
                break;
            }
            if futex.phy_addr.0 == phy_addr.0 && futex.value != word_value {
                wake_count -= 1;
                let mut thread = self.get_thread_mut(futex.pid);
                assert_eq!(thread.state, ThreadState::FutexWaiting);
                thread.state = ThreadState::Runnable;
                thread.sleep_deadline = 0;

                active_futexes.swap_remove(i);
            } else {
                i += 1;
            }
        }
    }

    pub unsafe fn alloc_port(&self, pid: Pid, recv_handle: u64) -> Arc<RefCell<Port>> {
        println!(
            " user: Allocating port for thread {} with recv_handle {}",
            pid, recv_handle
        );
        Arc::new(RefCell::new(Port {
            recv_pid: pid,
            recv_handle,
            buffer_len: 0,
            buffer: PageSlice::null(),
        }))
    }

    pub unsafe fn alloc_region(&self, data: PageSlice, owned: bool) -> Arc<RefCell<Region>> {
        Arc::new(RefCell::new(Region { data, owned }))
    }

    /// Allocates a contiguous block of handle ids
    /// Returns the first handle id in the block
    /// Must allocate at least 1 handle id
    pub fn reserve_handle_ids(&mut self, count: usize) -> u64 {
        assert!(count > 0, "Cannot allocate 0 handle ids");
        let res = self.next_handle_id + 1;
        self.next_handle_id += count as u64;
        res
    }

    pub fn reserve_handle_id(&mut self) -> u64 {
        self.reserve_handle_ids(1)
    }

    pub unsafe fn thread_phy_map(
        &self,
        pid: Pid,
        phy_addr: u64,
        len: usize,
        flags: PhyMapFlags,
    ) -> PhyMapResult {
        let thread = self.get_thread_mut(pid);

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
                virt_addr: thread.mm.borrow_mut().page_table.as_mut().vmap(
                    phy_addr,
                    len as usize,
                    page_flags,
                ) as u64,
                phy_addr: phy_addr.0 as u64,
            }
        } else {
            // Must map a specific physical address
            PhyMapResult {
                virt_addr: thread.mm.borrow_mut().page_table.as_mut().vmap(
                    PhyAddr(phy_addr as usize),
                    len,
                    page_flags,
                ) as u64,
                phy_addr: phy_addr as u64,
            }
        }
    }
}

pub type ThreadMutLockGuard<'a> = RefMut<'a, Thread>;

pub struct PhyMapResult {
    pub virt_addr: u64,
    pub phy_addr: u64,
}

pub unsafe fn handle_timer_tick(e: &mut ExceptionContext) {
    ThreadManager::get_global().schedule(e);
}

pub unsafe fn start() {
    println!(" user: Starting usermode");

    ThreadManager::init_global();
    let mut thread = ThreadManager::get_global().get_current_thread();
    thread.map_init_binary();
    Thread::enter(thread);
}
