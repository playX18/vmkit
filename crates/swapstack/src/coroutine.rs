//! # coroutine
//!
//! A simple wrapper on top of `swapstack()` to implement stackful coroutines.
//!  
//! They always execute on a separate stack and allocation of stack is managed by the library itself.

use std::{mem::ManuallyDrop, panic::AssertUnwindSafe, ptr::null_mut};

use crate::{
    raw::{ontop_swapstack, swapstack},
    stack::{Stack, Transfer},
};

#[repr(C)]
struct CoroutineForceUnwind {
    to: *mut Stack,
}

unsafe impl Send for CoroutineForceUnwind {}

extern "C-unwind" fn coroutine_unwind(t: Transfer) -> Transfer {
    std::panic::resume_unwind(Box::new(CoroutineForceUnwind { to: t.stack }))
}

extern "C-unwind" fn coroutine_exit<F: FnOnce(Coroutine) -> Coroutine>(t: Transfer) -> Transfer {
    let rec = t.data as *mut CoroutineRecord<F>;

    unsafe {
        std::ptr::drop_in_place(rec);
    }

    Transfer {
        stack: null_mut(),
        data: null_mut(),
    }
}

extern "C-unwind" fn coroutine_entry<F: FnOnce(Coroutine) -> Coroutine>(
    mut t: Transfer,
) -> Transfer {
    {
        let rec = t.data as *mut CoroutineRecord<F>;

        let result = std::panic::catch_unwind(AssertUnwindSafe(|| unsafe {
            t = swapstack((*rec).stack, t.stack, null_mut());
            t.stack = (*rec).run(t.stack);
        }));

        match result {
            Ok(_) => (),
            Err(e) => {
                if let Some(unwind) = e.downcast_ref::<CoroutineForceUnwind>() {
                    println!("oh wow~!");
                    t.stack = unwind.to;
                } else {
                    unreachable!("unhandled panic in coroutine: {:?}", e);
                }
            }
        }

        unsafe {
            ontop_swapstack((*rec).stack, t.stack, rec as _, coroutine_exit::<F>);
            unreachable!("coroutine already terminated");
        }
    }
}

extern "C-unwind" fn coroutine_ontop<F>(t: Transfer) -> Transfer
where
    F: FnOnce(Coroutine) -> Coroutine,
{
    unsafe {
        let f = ManuallyDrop::take(&mut *(t.data as *mut ManuallyDrop<F>));
        let mut c = f(Coroutine { stack: t.stack });

        Transfer {
            stack: std::mem::replace(&mut c.stack, null_mut()),
            data: null_mut(),
        }
    }
}

#[repr(C, align(16))]
struct CoroutineRecord<F> {
    stack: *mut Stack,
    callback: Option<F>,
}

impl<F> Drop for CoroutineRecord<F> {
    fn drop(&mut self) {
        unsafe {
            let _ = Box::from_raw(self.stack);
        }
    }
}

impl<F> CoroutineRecord<F>
where
    F: FnOnce(Coroutine) -> Coroutine,
{
    fn run(&mut self, stack: *mut Stack) -> *mut Stack {
        {
            let mut f = (self.callback.take().unwrap())(Coroutine { stack });
            std::mem::replace(&mut f.stack, null_mut())
        }
    }
}

#[repr(C)]
pub struct Coroutine {
    stack: *mut Stack,
}

impl Coroutine {
    pub fn new<F>(f: F) -> Self
    where
        F: FnOnce(Coroutine) -> Coroutine,
    {
        let stack = Box::into_raw(Box::new(Stack::new(None)));
        unsafe {
            let mut cur = Stack::from_native();
            // we can push record right before initializing stack just fine,
            // it cannot break SP
            let record = (*stack).push::<CoroutineRecord<F>>();

            (*stack).initialize(coroutine_entry::<F>, null_mut());
            record.write(CoroutineRecord {
                stack,
                callback: Some(f),
            });
            let t = swapstack(&mut cur, (*record).stack, record as _);

            Self { stack: t.stack }
        }
    }

    pub fn resume(mut self) -> Self {
        assert!(!self.stack.is_null());
        unsafe {
            let mut cur = Stack::from_native();
            Coroutine {
                stack: swapstack(
                    &mut cur,
                    std::mem::replace(&mut self.stack, null_mut()),
                    null_mut(),
                )
                .stack,
            }
        }
    }

    pub fn resume_with<F>(mut self, f: F) -> Self
    where
        F: FnOnce(Coroutine) -> Coroutine,
    {
        assert!(!self.stack.is_null());
        unsafe {
            let mut cur = Stack::from_native();
            let p = &f as *const _ as *mut ();
            std::mem::forget(f);
            Coroutine {
                stack: ontop_swapstack(
                    &mut cur,
                    std::mem::replace(&mut self.stack, null_mut()),
                    p,
                    coroutine_ontop::<F>,
                )
                .stack,
            }
        }
    }
}

impl Drop for Coroutine {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return; // when thread panicking do not do anything
        }

        if !self.stack.is_null() {
            // cannot unwind native stack :(
            if unsafe { (*self.stack).is_native() } {
                return;
            }
            let mut cur = Stack::from_native();
            unsafe {
                ontop_swapstack(&mut cur, self.stack, null_mut(), coroutine_unwind);
            }
        }
    }
}
