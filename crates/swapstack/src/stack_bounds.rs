//! # StackBounds
//!
//! A helper struct to fetch stack-bounds of thread stack. This is only applicable
//! to threads running on "native" stack.

use std::{mem::MaybeUninit, ptr::null_mut};

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct StackBounds {
    origin: *mut u8,
    bound: *mut u8,
}

#[cfg(target_os = "macos")]
impl StackBounds {
    unsafe fn new_thread_stack_bounds(handle: libc::pthread_t) {
        let origin = libc::pthread_get_stackaddr_np(handle);
        let size = libc::pthread_get_stacksize_np(thread);
        let bound = origin.sub(size);
        Self { origin, bound }
    }

    unsafe fn current_thread_stack_bounds_internal() -> Self {
        if libc::pthread_main_np() {
            let origin = libc::pthread_get_stackaddr_np(libc::pthread_self());
            let mut limit = std::mem::MaybeUninit::<libc::rlimit>::uninit();
            libc::getrlimit(libc::RLIMIT_STACK, limit.as_mut_ptr());
            let limit = limit.assume_init();
            let mut size = limit.rlim_cur as usize;
            if size == libc::RLIM_INFINITY as usize {
                size = 8 * 1024 * 1024;
            }

            let bound = origin.sub(size);

            Self { origin, bound }
        }
    }
}

#[cfg(target_os = "openbsd")]
impl StackBounds {
    unsafe fn new_thread_stack_bounds(handle: libc::pthread_t) -> Self {
        let mut stack = std::mem::MaybeUninit::<libc::stack_t>::uninit();
        libc::pthread_stackseg_np(handle, stack.as_mut_ptr());
        let stack = stack.assume_init();
        let origin = stack.ss_sp;
        let bound = origin.sub(stack.ss_size as usize);
        Self { origin, bound }
    }
}

#[cfg(all(not(target_os = "macos"), not(target_os = "openbsd")))]
impl StackBounds {
    unsafe fn new_thread_stack_bounds(handle: libc::pthread_t) -> Self {
        let mut bound = null_mut::<libc::c_void>();
        let mut stacksize = 0;

        let mut sattr = MaybeUninit::<libc::pthread_attr_t>::uninit();
        libc::pthread_attr_init(sattr.as_mut_ptr());
        let mut sattr = sattr.assume_init();

        libc::pthread_getattr_np(handle, &mut sattr);
        let _ = libc::pthread_attr_getstack(&sattr, &mut bound, &mut stacksize);
        let bound = bound.cast::<u8>();
        let origin = bound.add(stacksize);
        Self { origin, bound }
    }
}

#[cfg(not(target_os = "macos"))]
impl StackBounds {
    unsafe fn current_thread_stack_bounds_internal() -> Self {
        let ret = Self::new_thread_stack_bounds(libc::pthread_self());

        #[cfg(target_os = "linux")]
        {
            if libc::getpid() as i64 == libc::syscall(libc::SYS_gettid) {
                let origin = ret.origin;
                let mut limit = MaybeUninit::<libc::rlimit>::uninit();
                libc::getrlimit(libc::RLIMIT_STACK, limit.as_mut_ptr());
                let limit = limit.assume_init();
                let mut size = limit.rlim_cur;
                if size == libc::RLIM_INFINITY {
                    size = 8 * 1024 * 1024;
                }
                size -= libc::sysconf(libc::_SC_PAGE_SIZE) as u64;
                let bound = origin.sub(size as usize);
                return Self { origin, bound };
            }
        }

        ret
    }
}

thread_local! {
    static NATIVE_STACK_BOUNDS: StackBounds = unsafe { StackBounds::current_thread_stack_bounds_internal() };
}

impl StackBounds {
    /// Fetch current thread stack bounds.
    ///
    /// # Note
    ///
    /// The return of this function is stack bounds for *native* stack allocated by libc and not by VMKit.
    pub fn current() -> StackBounds {
        NATIVE_STACK_BOUNDS.with(|bounds| *bounds)
    }

    pub fn origin(&self) -> *mut u8 {
        self.origin
    }

    pub fn bound(&self) -> *mut u8 {
        self.bound
    }

    pub unsafe fn from_parts(bound: *mut u8, origin: *mut u8) -> Self {
        Self { bound, origin }
    }
}
