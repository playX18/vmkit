use crate::mm::slot::SlotExt;
use mmtk::util::{Address, ObjectReference};
use std::{
    mem::offset_of,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::{
    objectmodel::traits::{ScanSlots, TraceRefs},
    Runtime, SlotOf,
};

pub struct BasicLock {
    metadata: Address,
}

impl BasicLock {
    pub fn metadata(&self) -> Address {
        unsafe {
            Address::from_usize(
                Address::from_ref(&self.metadata).atomic_load::<AtomicUsize>(Ordering::Relaxed),
            )
        }
    }

    pub fn set_metadata(&self, meta: Address) {
        unsafe {
            Address::from_ref(&self.metadata)
                .atomic_store::<AtomicUsize>(meta.as_usize(), Ordering::Relaxed);
        }
    }
}

#[repr(C)]
pub struct BasicObjectLock {
    lock: BasicLock,
    obj: ObjectReference,
}

impl BasicObjectLock {
    pub const LOCK_OFFSET: usize = offset_of!(Self, lock);
    pub const OBJ_OFFSET: usize = offset_of!(Self, obj);

    pub fn object(&self) -> ObjectReference {
        self.obj
    }

    pub fn set_object(&mut self, obj: ObjectReference) {
        self.obj = obj;
    }

    pub fn lock(&self) -> &BasicLock {
        &self.lock
    }
}

impl<R: Runtime> TraceRefs<R> for BasicObjectLock {
    fn trace(&mut self, tracer: &mut crate::mm::scanning::Tracer<R>) {
        self.obj = tracer.trace_object_reference(self.obj);
    }
}

impl<R: Runtime> ScanSlots<R> for BasicObjectLock {
    fn scan(&self, visitor: &mut crate::mm::scanning::Visitor<R>) {
        visitor.visit_slot(SlotOf::<R>::from_pointer(
            Address::from_ref(&self.obj).to_mut_ptr(),
        ));
    }
}
