use std::{
    cell::UnsafeCell,
    hash::Hash,
    mem::{offset_of, ManuallyDrop},
    ptr::{addr_of_mut, NonNull},
    sync::atomic::{AtomicUsize, Ordering},
};

pub struct Inner<T> {
    pub rc: AtomicUsize,
    pub data: ManuallyDrop<T>,
}

pub struct P<T> {
    inner: NonNull<Inner<T>>,
}

impl<T> P<T> {
    pub fn new(data: T) -> Self {
        let x: Box<_> = Box::new(Inner {
            rc: AtomicUsize::new(1),
            data: ManuallyDrop::new(data),
        });

        Self {
            inner: unsafe { NonNull::new_unchecked(Box::into_raw(x)) },
        }
    }

    pub fn ptr_eq(this: &Self, other: &Self) -> bool {
        this.inner.as_ptr() == other.inner.as_ptr()
    }

    pub const unsafe fn from_inner(inner: *mut Inner<T>) -> Self {
        Self {
            inner: NonNull::new_unchecked(inner),
        }
    }

    pub fn as_ptr(this: &Self) -> *const T {
        let ptr = this.inner.as_ptr();

        unsafe { addr_of_mut!((*ptr).data).cast() }
    }

    pub unsafe fn from_raw(ptr: *const T) -> Self {
        unsafe {
            let offset = data_offset::<T>();
            let p_ptr = ptr.byte_sub(offset) as *mut Inner<T>;
            Self::from_inner(p_ptr)
        }
    }

    fn inner(&self) -> &Inner<T> {
        unsafe { self.inner.as_ref() }
    }

    pub unsafe fn get_mut_unchecked(&mut self) -> &mut T {
        &mut self.inner.as_mut().data
    }

    unsafe fn drop_slow(&mut self) {
        // Destroy the data at this time, even though we must not free the box
        // allocation itself (there might still be weak pointers lying around).
        unsafe { std::ptr::drop_in_place(Self::get_mut_unchecked(self)) };

        // Drop the weak ref collectively held by all strong references
        // Take a reference to `self.alloc` instead of cloning because 1. it'll
        // last long enough, and 2. you should be able to drop `Arc`s with
        // unclonable allocators
        let _ = Box::from_raw(self.inner.as_ptr());
    }
}

impl<T> std::ops::Deref for P<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner().data
    }
}

impl<T> Clone for P<T> {
    fn clone(&self) -> Self {
        self.inner().rc.fetch_add(1, Ordering::Release);

        unsafe { Self::from_inner(self.inner.as_ptr()) }
    }
}

impl<T> Drop for P<T> {
    fn drop(&mut self) {
        if self.inner().rc.fetch_sub(1, Ordering::Release) != 1 {
            return;
        }

        unsafe {
            self.drop_slow();
        }
    }
}

pub struct StaticUnsafeWrap<T>(UnsafeCell<T>);

impl<T> StaticUnsafeWrap<T> {
    pub const unsafe fn new(value: T) -> Self {
        Self(UnsafeCell::new(value))
    }

    pub const unsafe fn get(&self) -> *mut T {
        self.0.get()
    }
}

unsafe impl<T: Send> Send for StaticUnsafeWrap<T> {}
unsafe impl<T: Sync> Sync for StaticUnsafeWrap<T> {}

unsafe impl<T: Send> Send for P<T> {}
unsafe impl<T: Sync> Sync for P<T> {}

/// A macro to create statically allocated reference counted pointers.
///
/// This is a replacement for LazyLock which does not require expensive checking
/// at use site whether the value is initialized or not, just use the value as is.
///
/// The init expr must be a constant expression, we do not use `#[ctor]` or any other hacks here.
///
/// Example:
///
/// ```rust
/// use mu_utils::{static_p, rc::*};
///
/// static_p! {
///     static ref X: i32 = 42;
/// }
///
/// let x = X.clone();
///
/// println!("{}", *x);
///
/// ```
#[macro_export]
macro_rules! static_p {
    (
        $(
            $v: vis static ref $name: ident : $t: ty = $init: expr;
        )*

    ) => {
        $(
            paste::paste! {
                #[doc(hidden)]
                #[allow(unused_imports)]
                pub mod [<__impl_static_ $name: lower>] {
                    use super::*;
                    pub(super) static INNER: $crate::rc::StaticUnsafeWrap<$crate::rc::Inner<$t>> = unsafe { $crate::rc::StaticUnsafeWrap::new($crate::rc::Inner {
                        rc: std::sync::atomic::AtomicUsize::new(1),
                        data: std::mem::ManuallyDrop::new($init)
                    }) };
                }
                $v static $name: $crate::rc::P<$t> = unsafe { $crate::rc::P::<$t>::from_inner([<__impl_static_ $name:lower>]::INNER.get())  };
            }

        )*
    };
}

const fn data_offset<T>() -> usize {
    offset_of!(Inner<T>, data)
}

static_p!(
    pub static ref X: i32 = 42;
);

use std::fmt;

impl<T: fmt::Display> fmt::Display for P<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (**self).fmt(f)
    }
}

impl<T: fmt::Debug> fmt::Debug for P<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (**self).fmt(f)
    }
}

impl<T: fmt::Pointer> fmt::Pointer for P<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (**self).fmt(f)
    }
}

impl<T: PartialEq> PartialEq for P<T> {
    fn eq(&self, other: &Self) -> bool {
        (**self).eq(&**other)
    }
}

impl<T: Eq> Eq for P<T> {}

impl<T: PartialOrd> PartialOrd for P<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        (**self).partial_cmp(&**other)
    }
}

impl<T: Ord> Ord for P<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (**self).cmp(&**other)
    }
}

impl<T: Hash> Hash for P<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (**self).hash(state);
    }
}
