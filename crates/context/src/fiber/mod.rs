use std::{
    any::Any,
    cell::{Cell, UnsafeCell},
    marker::PhantomData,
    mem::ManuallyDrop,
    panic::AssertUnwindSafe,
    ptr::null_mut,
};

use crate::{
    internal::fcontext::PlatformFContextTop,
    stack_context::{MmapStack, StackStorage},
};

#[cfg(feature = "fcontext")]
pub mod fcontext;

mod inner {
    #[cfg(feature = "fcontext")]
    pub use super::fcontext::*;
}

pub struct Fiber<'a, Resume, Yield, Return> {
    inner: UnsafeCell<ManuallyDrop<inner::Fiber>>,
    done: Cell<bool>,
    marker: PhantomData<&'a (Resume, Yield, Return)>,
}

impl<'a, Resume, Yield, Return> Fiber<'a, Resume, Yield, Return> {
    pub fn new<F>(f: F) -> Self
    where
        F: FnOnce(Resume, &mut Suspend<'a, Resume, Yield, Return>) -> Return,
    {
        unsafe { Self::from_stack(StackStorage::Mmap(MmapStack::new(32 * 1024).unwrap()), f) }
    }

    pub unsafe fn from_stack<F>(stack: StackStorage, f: F) -> Self
    where
        F: FnOnce(Resume, &mut Suspend<'a, Resume, Yield, Return>) -> Return,
    {
        Self {
            inner: UnsafeCell::new(ManuallyDrop::new(inner::Fiber::from_parts(
                stack,
                move |f_, arg| {
                    let mut suspend = Suspend {
                        inner: UnsafeCell::new(ManuallyDrop::new(f_)),
                        dest: Cell::new(arg as _),
                        marker: PhantomData,
                    };
                    let x = match std::mem::replace(
                        unsafe { suspend.dest.get().as_mut().unwrap() },
                        RunResult::Executing,
                    ) {
                        RunResult::Resuming(x) => x,
                        _ => unreachable!(),
                    };
                    let ret = std::panic::catch_unwind(AssertUnwindSafe(|| f(x, &mut suspend)));
                    unsafe {
                        suspend.dest.get().write(match ret {
                            Ok(ret) => RunResult::Returned(ret),
                            Err(err) if !err.is::<inner::ForcedUnwind>() => {
                                RunResult::Panicked(err)
                            }
                            // force-unwind: continue unwinding and we'll eventually drop the fiber.
                            Err(err) => std::panic::resume_unwind(err),
                        });
                        ManuallyDrop::into_inner(suspend.inner.get().read())
                    }
                },
            ))),
            done: Cell::new(false),
            marker: PhantomData,
        }
    }

    pub fn resume(&self, value: Resume) -> Result<Return, Yield> {
        assert!(!self.done.replace(true), "cannot resume a finished fiber");
        let mut result = RunResult::Resuming(value);
        unsafe {
            let inner = ManuallyDrop::into_inner(self.inner.get().read());
            let (new, _) = inner.resume(&mut result as *mut RunResult<Resume, Yield, Return> as _);
            self.inner.get().write(ManuallyDrop::new(new));
            match result {
                RunResult::Resuming(_) | RunResult::Executing => unreachable!(),
                RunResult::Yield(y) => {
                    self.done.set(false);
                    Err(y)
                }
                RunResult::Returned(r) => Ok(r),
                RunResult::Panicked(p) => std::panic::resume_unwind(p),
            }
        }
    }

    pub fn fcontext_top(&self) -> *mut PlatformFContextTop {
        unsafe { self.inner.get().as_ref().unwrap().raw().cast() }
    }
}

impl<'a, Resume, Yield, Return> Drop for Fiber<'a, Resume, Yield, Return> {
    fn drop(&mut self) {
        unsafe {
            self.inner.get().read();
        }
    }
}

pub struct Suspend<'a, Resume, Yield, Return> {
    inner: UnsafeCell<ManuallyDrop<inner::Fiber>>,
    dest: Cell<*mut RunResult<Resume, Yield, Return>>,
    marker: PhantomData<&'a (Resume, Yield, Return)>,
}

impl<'a, Resume, Yield, Return> Suspend<'a, Resume, Yield, Return> {
    pub fn suspend(&self, value: Yield) -> Resume {
        unsafe {
            *self.dest.get().as_mut().unwrap() = RunResult::Yield(value);
            let inner = self.inner.get().read();
            let (new, dest) = ManuallyDrop::into_inner(inner).resume(null_mut());
            self.inner.get().write(ManuallyDrop::new(new));
            self.dest.set(dest as _);
            match std::mem::replace(&mut *self.dest.get(), RunResult::Executing) {
                RunResult::Resuming(val) => val,

                _ => unreachable!(),
            }
        }
    }

    pub fn fcontext_top(&self) -> *mut PlatformFContextTop {
        unsafe { self.inner.get().as_ref().unwrap().raw().cast() }
    }
}

enum RunResult<Resume, Yield, Return> {
    Executing,
    Resuming(Resume),
    Yield(Yield),
    Returned(Return),
    Panicked(Box<dyn Any + Send>),
}
