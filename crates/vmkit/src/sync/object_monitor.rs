use mmtk::util::{Address, ObjectReference, VMThread};
use std::{
    ptr::null_mut,
    sync::atomic::{AtomicBool, AtomicIsize, AtomicPtr, AtomicU64, AtomicU8, AtomicUsize},
};

pub const TS_UNDEF: u8 = 0;
pub const TS_READY: u8 = 1;
pub const TS_RUN: u8 = 2;
pub const TS_WAIT: u8 = 3;
pub const TS_ENTER: u8 = 4;
pub const TS_CXQ: u8 = 5;

pub struct ObjectWaiter {
    next: AtomicPtr<Self>,
    prev: AtomicPtr<Self>,
    thread: VMThread,
    notified_tid: AtomicU64,
    notified: AtomicBool,
    tstate: AtomicU8,
    active: AtomicBool,
}

impl ObjectWaiter {
    fn new(current: VMThread) -> Self {
        Self {
            next: AtomicPtr::new(null_mut()),
            prev: AtomicPtr::new(null_mut()),
            active: AtomicBool::new(false),
            notified: AtomicBool::new(false),
            notified_tid: AtomicU64::new(u64::MAX),
            thread: current,
            tstate: AtomicU8::new(TS_RUN),
        }
    }
}

pub struct ObjectMonitor {
    /// Backward object pointer
    object: Option<ObjectReference>,
    owner: AtomicUsize,
    /// thread id of the previous owner of the monitor
    /// Separate owner and next_om on different cache lines since
    /// both can have busy multi-threaded access. previous_owner_tid is only
    /// changed by ObjectMonitor::exit() so it is a good choice to share the
    /// cache line with owner.
    previous_owner_tid: AtomicU64,
    next_om: AtomicUsize,
    recursions: AtomicIsize,
}
