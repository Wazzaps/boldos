use crate::aarch64::exceptions::ExceptionContext;
use crate::drv::arm_gic::timer_get_absolute_time_ms;
use crate::drv::qemu_console::puts;
use crate::ipc::{HandleDataRef, PortHandle, PORT_MAX_MESSAGE_SIZE};
use crate::page_alloc::{add_memory_node, alloc_zeroed, PageSlice, PhyAddr, PAGE_SIZE};
use crate::threads::{PhyMapResult, ThreadManager};
use crate::{drv, print, println};
use core::arch::asm;
use core::mem::MaybeUninit;
use kernel_api::kernel_device::KernelDeviceId;
use kernel_api::{
    kernel_device, ControlThreadOp, CreateThreadFlags, FutexOp, KError, MemMapFlags, PhyMapFlags,
    Syscall,
};
use zerocopy::{FromBytes, FromZeros, Immutable, IntoBytes};

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
            let mgr = ThreadManager::get_global();

            let mut buf = [0u8; 256];
            let ptr = e.gpr[0];
            let len = e.gpr[1].min(buf.len() as u64);
            copy_from_user(ptr as usize, len as usize, &mut buf[..len as usize]);
            if mgr.last_log_was_newline {
                print!("[{}]", timer_get_absolute_time_ms());
            }
            if len > 0 {
                mgr.last_log_was_newline = buf[len as usize - 1] == b'\n';
            }
            puts(&buf[..len as usize]);
        }
        Syscall::PhyMap => {
            let phy_addr = e.gpr[0];
            let len = e.gpr[1];
            let flags = PhyMapFlags::from_bits_truncate(e.gpr[2]);
            let mgr = ThreadManager::get_global();

            let PhyMapResult {
                virt_addr,
                phy_addr,
            } = mgr.thread_phy_map(mgr.current_thread, phy_addr, len as usize, flags);
            e.gpr[0] = virt_addr;
            e.gpr[1] = phy_addr;
        }
        Syscall::MemMap => {
            let len = e.gpr[0];
            let flags = MemMapFlags::from_bits_truncate(e.gpr[1]);

            let virt_addr = ThreadManager::get_global()
                .get_current_thread()
                .mem_map(len as usize, flags);
            e.gpr[0] = virt_addr as u64;
        }
        Syscall::MemUnmap => {
            let virt_addr = e.gpr[0];
            let len = e.gpr[1];

            ThreadManager::get_global()
                .get_current_thread()
                .mem_unmap(virt_addr as usize, len as usize);

            e.gpr[0] = 0;
        }
        Syscall::DownloadMoreRam => {
            let phy_addr = e.gpr[0];
            let len = e.gpr[1];
            add_memory_node(PhyAddr(phy_addr as usize), len as usize);
            e.gpr[0] = 0;
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
                ThreadManager::get_global().start_schedule_timer();

                e.gpr[0] = 0;
                return;
            } else {
                e.gpr[0] = KError::InvalidArgument.into();
                return;
            }
        }
        Syscall::SleepMs => {
            let ms = e.gpr[0];
            let deadline: u64 = timer_get_absolute_time_ms() + ms;
            // println!(" user: Sleeping for {} ms", ms);
            let mgr = ThreadManager::get_global();
            mgr.get_current_thread().start_sleep(deadline);
            mgr.schedule(e);
        }
        Syscall::CreateThread => {
            let func = e.gpr[0];
            let data_ptr = e.gpr[1];
            let flags = CreateThreadFlags::from_bits_truncate(e.gpr[2]);

            let mgr = ThreadManager::get_global();
            let share_page_table = if flags.contains(CreateThreadFlags::SharePageTable) {
                Some(mgr.current_thread)
            } else {
                None
            };
            let share_handles = if flags.contains(CreateThreadFlags::ShareHandles) {
                Some(mgr.current_thread)
            } else {
                None
            };

            let (pid, mut thread) =
                mgr.create_thread(share_page_table.clone(), share_handles.clone());
            thread.regs.gpr[0] = data_ptr;
            thread.regs.pc = func;
            if share_page_table.is_none() {
                // TODO: Load executables in user mode
                thread.map_init_binary();
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
                    mgr.get_thread(pid).pause();
                    if pid == mgr.current_thread {
                        todo!("Suspended own thread, need to switch to next thread");
                    }
                    e.gpr[0] = 0;
                    return;
                }
                ControlThreadOp::Resume => {
                    mgr.get_thread(pid).resume();
                    e.gpr[0] = 0;
                    return;
                }
            }
        }
        Syscall::VirtToPhys => {
            let virt_addr = e.gpr[0];
            let thread = ThreadManager::get_global().get_current_thread();
            if let Some(phy_addr) = thread.virt_to_phys(virt_addr as usize) {
                e.gpr[0] = phy_addr.0 as u64;
            } else {
                e.gpr[0] = KError::InvalidAddress.into();
            }
            return;
        }
        Syscall::Futex => {
            let word = e.gpr[0];
            let op = FutexOp::from_bits(e.gpr[1]).expect("Invalid futex operation");
            let val = e.gpr[2] as u32;
            let timeout_micros = e.gpr[3] as u64;
            let mgr = ThreadManager::get_global();

            if op.bits() == FutexOp::WAKE.bits() {
                // TODO: all of this is only atomic because we are single core. Use proper spinlocks.

                let thread = mgr.get_current_thread();
                let Some(phy_addr) = thread.virt_to_phys(word as usize) else {
                    e.gpr[0] = KError::InvalidAddress.into();
                    return;
                };
                println!(" user: Futex: {:#x?}", phy_addr);

                assert!(!mgr.futexes_locked.get());
                mgr.futexes_locked.set(true);

                let mut word_value: u32 = 0;
                copy_from_user(word as usize, 4, word_value.as_mut_bytes());
                mgr.wake_futex_waiters(phy_addr, word_value, val);

                assert!(mgr.futexes_locked.get());
                mgr.futexes_locked.set(false);
            } else if op.bits() == FutexOp::WAIT.bits() {
                // TODO: all of this is only atomic because we are single core. Use proper spinlocks.

                let mut thread = mgr.get_current_thread();
                let Some(phy_addr) = thread.virt_to_phys(word as usize) else {
                    e.gpr[0] = KError::InvalidAddress.into();
                    return;
                };
                println!(" user: Futex: {:#x?}", phy_addr);

                assert!(!mgr.futexes_locked.get());
                mgr.futexes_locked.set(true);

                let mut word_value: u32 = 0;
                copy_from_user(word as usize, 4, word_value.as_mut_bytes());
                if word_value != val {
                    mgr.futexes_locked.set(false);
                    e.gpr[0] = KError::TryAgain.into();
                    return;
                }
                mgr.add_futex_waiter(phy_addr, mgr.current_thread, val);
                thread.start_futex_wait(timeout_micros);

                assert!(mgr.futexes_locked.get());
                mgr.futexes_locked.set(false);

                drop(thread);
                mgr.schedule(e);
            } else {
                e.gpr[0] = KError::InvalidArgument.into();
                return;
            }
        }
        Syscall::HandleDuplicate => {
            todo!("Syscall::HandleDuplicate not implemented")
        }
        Syscall::HandleClose => {
            let handle_id = e.gpr[0];
            let mgr = ThreadManager::get_global();
            let thread = mgr.get_current_thread();
            thread.remove_handle(handle_id, mgr);
            e.gpr[0] = 0;
            return;
        }
        Syscall::PortCreate => {
            let mgr = ThreadManager::get_global();

            assert!(!mgr.ipc_locked.get());
            mgr.ipc_locked.set(true);

            let rx_id = mgr.next_handle_id;
            let tx_id = mgr.next_handle_id + 1;
            mgr.next_handle_id += 2;

            let port_ptr = mgr.alloc_port(mgr.current_thread, rx_id);

            let thread = mgr.get_current_thread();
            thread.add_handle(
                rx_id,
                PortHandle::FLAG_RECV,
                HandleDataRef::Port(&PortHandle { port: port_ptr }),
            );
            thread.add_handle(
                tx_id,
                PortHandle::FLAG_SEND,
                HandleDataRef::Port(&PortHandle { port: port_ptr }),
            );
            drop(thread);

            assert!(mgr.ipc_locked.get());
            mgr.ipc_locked.set(false);

            e.gpr[0] = rx_id as u64;
            e.gpr[1] = tx_id as u64;
            return;
        }
        Syscall::PortRecv => {
            let port_handle_id = e.gpr[0];
            let _flags = e.gpr[1];
            let bytes_ptr = e.gpr[2];
            let bytes_len = e.gpr[3] as usize;
            let _handles_ptr = e.gpr[4];
            let handles_len = e.gpr[5];
            let mgr = ThreadManager::get_global();

            assert!(handles_len == 0, "Handle receiving not supported yet");
            assert!(
                bytes_len <= PORT_MAX_MESSAGE_SIZE,
                "message bytes buffer too large: {bytes_len}"
            );

            assert!(!mgr.ipc_locked.get());
            mgr.ipc_locked.set(true);

            let thread = mgr.get_current_thread();
            let handle = thread.find_handle(port_handle_id);
            let mut had_message = false;
            let mut found_handle = false;
            let mut message_len = 0;
            if !handle.is_null() {
                match (*handle).data() {
                    HandleDataRef::Port(port) => {
                        assert_eq!((*handle).flags, PortHandle::FLAG_RECV);
                        if (*port.port).buffer.as_ptr().is_null() {
                            // No data
                        } else {
                            had_message = true;
                            message_len = (*port.port).buffer_len;
                            let copy_len = bytes_len.min(message_len);
                            copy_to_user(
                                bytes_ptr as usize,
                                copy_len,
                                &(*port.port).buffer.as_slice()[..copy_len],
                            );
                            // Deallocate the buffer
                            (*port.port).buffer = PageSlice::null();
                        }
                        found_handle = true;
                    }
                    _ => todo!("Receiving on non-port handle"),
                }
            }

            drop(thread);

            assert!(mgr.ipc_locked.get());
            mgr.ipc_locked.set(false);

            if !found_handle {
                e.gpr[0] = KError::InvalidHandle.into();
            } else if !had_message {
                e.gpr[0] = KError::PortEmpty.into();
            } else {
                e.gpr[0] = message_len as u64;
            }
            return;
        }
        Syscall::PortSend => {
            let port_handle_id = e.gpr[0];
            let _flags = e.gpr[1];
            let bytes_ptr = e.gpr[2];
            let bytes_len = e.gpr[3] as usize;
            let _handles_ptr = e.gpr[4];
            let handles_len = e.gpr[5];
            let mgr = ThreadManager::get_global();

            assert!(handles_len == 0, "Handle sending not supported yet");
            assert!(
                bytes_len <= PORT_MAX_MESSAGE_SIZE,
                "message bytes buffer too large: {bytes_len}"
            );

            assert!(!mgr.ipc_locked.get());
            mgr.ipc_locked.set(true);

            let thread = mgr.get_current_thread();
            let handle = thread.find_handle(port_handle_id);
            let mut is_full = false;
            let mut found_handle = false;
            if !handle.is_null() {
                match (*handle).data() {
                    HandleDataRef::Port(port) => {
                        assert!(
                            (*handle).flags == PortHandle::FLAG_SEND
                                || (*handle).flags == PortHandle::FLAG_SEND_ONCE
                        );
                        if (*port.port).buffer.as_ptr().is_null() {
                            // We can send data
                            let mut buffer = alloc_zeroed(bytes_len.div_ceil(PAGE_SIZE));
                            copy_from_user(
                                bytes_ptr as usize,
                                bytes_len,
                                &mut buffer.as_mut_slice()[..bytes_len],
                            );
                            (*port.port).buffer = buffer;
                            (*port.port).buffer_len = bytes_len as usize;
                        } else {
                            is_full = true;
                        }
                        found_handle = true;
                    }
                    _ => todo!("Sending on non-port handle"),
                }
            }

            drop(thread);

            assert!(mgr.ipc_locked.get());
            mgr.ipc_locked.set(false);

            if !found_handle {
                e.gpr[0] = KError::InvalidHandle.into();
            } else if is_full {
                e.gpr[0] = KError::PortFull.into();
            } else {
                e.gpr[0] = 0;
            }
            return;
        }
        Syscall::RegionCreateVirtual => {
            todo!("Syscall::RegionCreateVirtual not implemented")
        }
        Syscall::RegionCreatePhysical => {
            todo!("Syscall::RegionCreatePhysical not implemented")
        }
        Syscall::RegionRead => {
            todo!("Syscall::RegionRead not implemented")
        }
        Syscall::RegionWrite => {
            todo!("Syscall::RegionWrite not implemented")
        }
        Syscall::MmCreate => {
            todo!("Syscall::MmCreate not implemented")
        }
        Syscall::MmModify => {
            todo!("Syscall::MmModify not implemented")
        }
        Syscall::WaiterCreate => {
            todo!("Syscall::WaiterCreate not implemented")
        }
        Syscall::WaiterWait => {
            todo!("Syscall::WaiterWait not implemented")
        }
    }
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

unsafe fn copy_to_user(user_pointer: usize, user_len: usize, source: &[u8]) {
    assert_eq!(user_len, source.len());
    for i in 0..user_len {
        let value = source[i] as u32;
        asm!("strb {0:w}, [{1}]", in(reg) value, in(reg) user_pointer + i);
    }
}

unsafe fn copy_val_from_user<T: FromBytes + IntoBytes>(user_pointer: usize) -> T {
    let mut value: MaybeUninit<T> = MaybeUninit::uninit();
    copy_from_user(
        user_pointer,
        size_of::<T>() as usize,
        core::slice::from_raw_parts_mut(value.as_mut_ptr() as *mut u8, size_of::<T>()),
    );
    value.assume_init()
}

unsafe fn copy_val_to_user<T: IntoBytes + Immutable>(user_pointer: usize, value: &T) {
    copy_to_user(user_pointer, size_of::<T>() as usize, value.as_bytes());
}
