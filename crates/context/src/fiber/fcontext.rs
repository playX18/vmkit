use std::{panic::AssertUnwindSafe, ptr::null_mut};

use crate::{
    internal::fcontext::{jump_fcontext, make_fcontext, ontop_fcontext, FContext, Transfer},
    stack_context::{MmapStack, StackStorage},
};

pub struct ForcedUnwind(FContext);

unsafe impl Send for ForcedUnwind {}

pub struct FiberRecord<F> {
    #[allow(dead_code)]
    stack: StackStorage,
    callback: Option<F>,
}

impl<F> FiberRecord<F> {
    fn run(&mut self, fctx: FContext, arg: *mut ()) -> FContext
    where
        F: FnOnce(Fiber, *mut ()) -> Fiber,
    {
        let f = self.callback.take().unwrap();
        let mut result = f(Fiber { fctx }, arg);
        std::mem::replace(&mut result.fctx, null_mut())
    }
}

extern "C-unwind" fn fiber_force_unwind(t: Transfer) -> Transfer {
    std::panic::resume_unwind(Box::new(ForcedUnwind(t.fctx)))
}

extern "C-unwind" fn fiber_start<F: FnOnce(Fiber, *mut ()) -> Fiber>(mut t: Transfer) {
    let rec = t.data as *mut FiberRecord<F>;
    unsafe {
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            t = jump_fcontext(t.fctx, null_mut());
            t.fctx = (*rec).run(t.fctx, t.data);
        }));

        match result {
            Ok(_) => (),
            Err(e) if e.is::<ForcedUnwind>() => {
                let f = e.downcast_ref::<ForcedUnwind>().unwrap();
                t.fctx = f.0;
            }

            Err(err) => {
                panic!("uncatched panic in fiber: {:?}", err);
            }
        }

        ontop_fcontext(t.fctx, rec as _, fiber_exit::<F>);
        unreachable!("fiber is already dead");
    }
}

extern "C-unwind" fn fiber_exit<F: FnOnce(Fiber, *mut ()) -> Fiber>(t: Transfer) -> Transfer {
    let rec = t.data as *mut FiberRecord<F>;

    unsafe {
        std::ptr::drop_in_place(rec);
    }

    Transfer {
        data: null_mut(),
        fctx: null_mut(),
    }
}

extern "C-unwind" fn fiber_ontop<F: FnOnce(Fiber) -> Fiber>(t: Transfer) -> Transfer {
    let p = t.data as *mut Option<F>;

    unsafe {
        let p = &mut *p;
        let f = p.take().unwrap();
        let mut c = f(Fiber { fctx: t.fctx });

        Transfer {
            fctx: std::mem::replace(&mut c.fctx, null_mut()),
            data: null_mut(),
        }
    }
}

pub struct Fiber {
    fctx: FContext,
}

impl Fiber {
    pub unsafe fn from_parts<F>(stack_storage: StackStorage, f: F) -> Self
    where
        F: FnOnce(Fiber, *mut ()) -> Fiber,
    {
        let control = stack_storage
            .top()
            .sub(size_of::<FiberRecord<F>>())
            .cast::<FiberRecord<F>>();
        let stack_top = control.byte_sub(64).cast::<u8>();
        let stack_bottom = stack_storage.top().sub(stack_storage.size());
        control.write(FiberRecord {
            callback: Some(f),
            stack: stack_storage,
        });

        let size = stack_top as usize - stack_bottom as usize;

        let fctx = make_fcontext(stack_top, size, fiber_start::<F>);

        Fiber {
            fctx: jump_fcontext(fctx, control.cast()).fctx,
        }
    }

    pub fn new<F>(f: F) -> Fiber
    where
        F: FnOnce(Self, *mut ()) -> Self,
    {
        let mmap = MmapStack::new(32 * 1024).expect("failed to allocate stack");
        unsafe { Self::from_parts(StackStorage::Mmap(mmap), f) }
    }

    pub fn resume(mut self, arg: *mut ()) -> (Fiber, *mut ()) {
        assert!(!self.fctx.is_null(), "fiber is dead");
        let t = unsafe { jump_fcontext(std::mem::replace(&mut self.fctx, null_mut()), arg as _) };
        (Self { fctx: t.fctx }, t.data)
    }

    pub fn resume_with<F>(mut self, f: F) -> Fiber
    where
        F: FnOnce(Self) -> Self,
    {
        assert!(!self.fctx.is_null(), "fiber is dead");
        let mut cb = Some(f);
        let ptr = &mut cb as *mut Option<F> as *mut u8;

        Self {
            fctx: unsafe {
                ontop_fcontext(
                    std::mem::replace(&mut self.fctx, null_mut()),
                    ptr,
                    fiber_ontop::<F>,
                )
                .fctx
            },
        }
    }

    pub fn raw(&self) -> FContext {
        self.fctx
    }

    pub unsafe fn from_raw(fctx: FContext) -> Self {
        Self { fctx }
    }

    pub fn into_raw(self) -> FContext {
        self.fctx
    }
}

impl Drop for Fiber {
    fn drop(&mut self) {
        if self.fctx.is_null() {
            return;
        }

        unsafe {
            ontop_fcontext(self.fctx, null_mut(), fiber_force_unwind);
        }
    }
}
