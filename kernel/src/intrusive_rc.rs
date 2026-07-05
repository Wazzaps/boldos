use crate::page_alloc::PageBox;

/// This is a reference counted [`PageBox`] implemented as an intrusive linked list.
///
/// To use it, first initialize your nodes using [`IntrusiveRc::uninit`], then use
/// [`IntrusiveRc::init_pinned`] on the first node (allocating the value), and
/// [`IntrusiveRc::init_pinned_sibling`] on the other nodes (effectively adding a reference).
///
/// It currently allows concurrent mutable access to the pointer, and isn't thread safe.
pub struct IntrusiveRc<T> {
    // TODO: locking?
    inner: *mut T,
    inner_len: usize,
    prev: *mut IntrusiveRc<T>,
    next: *mut IntrusiveRc<T>,
}

impl<T: 'static> IntrusiveRc<T> {
    pub fn uninit() -> Self {
        Self {
            inner: core::ptr::null_mut(),
            inner_len: 0,
            prev: core::ptr::null_mut(),
            next: core::ptr::null_mut(),
        }
    }

    /// # Safety
    ///
    /// After this call, self must not be moved until it's dropped.
    pub unsafe fn init_pinned(&mut self, inner: PageBox<T>) {
        let (inner, inner_len) = PageBox::into_raw(inner);
        self.inner = inner;
        self.inner_len = inner_len;
        self.prev = self;
        self.next = self;
    }

    /// # Safety
    ///
    /// After this call, self must not be moved until it's dropped.
    pub unsafe fn init_pinned_sibling(&mut self, sibling: &mut Self) {
        assert!(self.inner.is_null());
        assert!(!sibling.inner.is_null());
        self.inner = sibling.inner;
        self.inner_len = sibling.inner_len;
        self.prev = sibling;
        self.next = sibling.next;
        if sibling.next == sibling as *mut _ {
            sibling.prev = self;
            sibling.next = self;
        } else {
            sibling.next = self;
        }
    }

    // FIXME: Not exclusive with as_mut
    pub fn as_ref(&self) -> &T {
        assert!(!self.inner.is_null());
        unsafe { &*self.inner }
    }

    // FIXME: Not exclusive with as_ref and other as_mut calls
    pub fn as_mut(&mut self) -> &mut T {
        assert!(!self.inner.is_null());
        unsafe { &mut *self.inner }
    }

    pub fn as_ptr(&self) -> *const T {
        self.inner
    }

    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.inner
    }
}

impl<T> Drop for IntrusiveRc<T> {
    fn drop(&mut self) {
        if self.next.is_null() {
            // Uninitialized
            return;
        } else if self.prev == self.next {
            // We are the last owner, drop the value
            unsafe {
                drop(PageBox::from_raw(self.inner, self.inner_len));
            }
            return;
        } else {
            // We are not the last owner, unlink ourselves
            unsafe {
                (*self.next).prev = self.prev;
                (*self.prev).next = self.next;
            }
        }
    }
}
