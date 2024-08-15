/*use std::{
    cell::{RefCell, UnsafeCell},
    mem::{transmute, MaybeUninit},
    ptr::null,
    sync::{atomic::AtomicUsize, LazyLock},
};

use mmtk::{vm::slot::SimpleSlot, MMTKBuilder, MMTK};

use crate::{
    threads::{TLSData, Thread, Threads},
    MMTKLibAlloc, Runtime,
};

#[derive(Default)]
pub struct MockVM;

impl Runtime for MockVM {
    type Slot = SimpleSlot;
    type Thread = MockThread;
    fn current_thread() -> Arc<Self::Thread> {
        THREAD.with_borrow(|thread| thread.clone())
    }

    fn try_current_thread() -> Option<Arc<Self::Thread>> {
        Some(THREAD.with_borrow(|thread| thread.clone()))
    }

    fn out_of_memory(_thread: &'static Self::Thread, error: mmtk::util::alloc::AllocationError) {
        panic!("Out of memory: {:?}", error);
    }

    fn threads() -> &'static crate::threads::Threads<Self> {
        &THREADS
    }

    fn vm_live_bytes() -> usize {
        0
    }

    fn mmtk_instance() -> &'static mmtk::MMTK<crate::MMTKLibAlloc<Self>> {
        &MMTK
    }
}

static MMTK: LazyLock<MMTK<MMTKLibAlloc<MockVM>>> = LazyLock::new(|| MMTKBuilder::new().build());
static THREADS: LazyLock<Threads<MockVM>> = LazyLock::new(Threads::new);

pub struct MockThread {
    tls: MaybeUninit<UnsafeCell<TLSData<MockVM>>>,
    index_in_thread_list: AtomicUsize,
}
use std::sync::Arc;
thread_local! {
    static THREAD: RefCell<Arc<MockThread>> = RefCell::new(Arc::new(MockThread {
        index_in_thread_list: AtomicUsize::new(0),
        tls: MaybeUninit::uninit(),
    }));
}

impl Thread<MockVM> for MockThread {
    fn from_vm_mutator_thread(vmthread: mmtk::util::VMMutatorThread) -> &'static Self {
        unsafe { transmute(vmthread) }
    }

    fn to_vm_mutator_thread(&self) -> mmtk::util::VMMutatorThread {
        unsafe { transmute(self) }
    }

    fn index_in_thread_list(&self) -> usize {
        self.index_in_thread_list
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    fn set_index_in_thread_list(&self, ix: usize) {
        self.index_in_thread_list
            .store(ix, std::sync::atomic::Ordering::Relaxed)
    }

    fn tls(&self) -> &UnsafeCell<TLSData<MockVM>> {
        unsafe { self.tls.assume_init_ref() }
    }

    fn attach_gc_data(&self, tls: UnsafeCell<TLSData<MockVM>>) {
        unsafe {
            (self.tls.as_ptr() as *mut UnsafeCell<_>).write(tls);
        }
    }

    fn detach_gc_data(&self) -> UnsafeCell<TLSData<MockVM>> {
        unsafe { self.tls.assume_init_read() }
    }
}
*/
