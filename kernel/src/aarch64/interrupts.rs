use crate::set_msr_const;
use aarch64_cpu::registers::DAIF;
use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
};
use tock_registers::interfaces::{Readable, Writeable};

pub struct IrqMutex<T: ?Sized> {
    inner: UnsafeCell<T>,
}

impl<T> IrqMutex<T> {
    pub const fn new(value: T) -> Self {
        Self {
            inner: UnsafeCell::new(value),
        }
    }
}

impl<T: ?Sized> IrqMutex<T> {
    pub fn lock(&self) -> IrqGuard<T> {
        let prev_state = DAIF.get();
        unsafe { disable() };
        // assert_eq!(DAIF.get() & (1 << 7), 1 << 7);
        // assert_eq!(DAIF.get(), DAIF.get());
        IrqGuard {
            mutex: self,
            prev_state,
        }
    }
}

unsafe impl<T: ?Sized + Send> Send for IrqMutex<T> {}
unsafe impl<T: ?Sized + Send> Sync for IrqMutex<T> {}

pub struct IrqGuard<'a, T: ?Sized + 'a> {
    mutex: &'a IrqMutex<T>,
    prev_state: u64,
}

// pub fn irq_lock() -> IrqLock {
//     let prev_state = DAIF.get();
//     unsafe { disable() };
//     // assert_eq!(DAIF.get() & (1 << 7), 1 << 7);
//     // assert_eq!(DAIF.get(), DAIF.get());
//     IrqLock { prev_state }
// }

impl<T: ?Sized> Drop for IrqGuard<'_, T> {
    fn drop(&mut self) {
        DAIF.set(self.prev_state);
        // assert_eq!(DAIF.get(), self.prev_state);
    }
}

impl<T: ?Sized> Deref for IrqGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.mutex.inner.get() }
    }
}

impl<T: ?Sized> DerefMut for IrqGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.mutex.inner.get() }
    }
}

pub unsafe fn enable() {
    set_msr_const!(daifclr, 2);
}

pub unsafe fn disable() {
    set_msr_const!(daifset, 2);
}
