use std::{
    marker::PhantomData,
    sync::atomic::{AtomicUsize, Ordering},
};

use mmtk::{
    util::{alloc::AllocationError, Address, ObjectReference, VMThread},
    vm::{
        slot::{Slot, UnimplementedMemorySlice},
        ReferenceGlue, RootsWorkFactory, VMBinding,
    },
    MMTKBuilder, MMTK,
};
use options::mmtk_options;
use threads::Threads;
use thunks::Thunks;

use crate::{
    mm::scanning::VMScanning,
    objectmodel::{reference::SlotExt, vtable::VTable},
};

pub mod context;
pub mod options;
pub mod osr;
pub mod signals;
pub mod threads;
pub mod thunks;
pub mod unwind;

pub trait Runtime: 'static + Default + Send + Sync {
    type Slot: Slot + SlotExt;
    type VTable: VTable<Self>;
    type Thread: threads::Thread<Self>;

    /// An accessor for thread-local storage of current thread. You can simply use `thread_local!` and return
    /// pointer to it.
    fn current_thread() -> VMThread {
        threads::vmkit_current_thread()
    }
    fn out_of_memory(thread: VMThread, error: AllocationError);
    /// Act upon receiving null pointer access in VM code, never allowed
    /// to return as we're in signal handler and can't just re-run "null" access.
    ///
    /// Note: this handler is ran on stack that triggered Null access, you can swap it
    /// to a new stack if necessary, unwind the stack etc.
    fn null_pointer_access(ip: Address) -> !;

    /// Act upon receiving stack-overflow in VM code, never allowed
    /// to return as we're in signal handler and can't just re-run stack-overflow.
    fn stack_overflow(ip: Address, addr: Address) -> !;

    fn vm_live_bytes() -> usize {
        0
    }

    /// The fallback for object tracing. MMTk generally expects to find an object in one of MMTk's spaces (if it is allocated by MMTK),
    /// and apply the corresponding policy to trace the object. Tracing in MMTk means identifying whether we have encountered this object in the
    /// current GC. For example, for mark sweep, we will check if an object is marked, and if it is not yet marked, mark and enqueue the object
    /// for later scanning. For copying policies, copying also happens in this step. For example for MMTk's copying space, we will
    /// copy an object if it is in 'from space', and enqueue the copied object for later scanning.
    ///
    /// If a binding would like to trace objects that are not allocated by MMTk and are not in any MMTk space, they can override this method.
    /// They should check whether the object is encountered before in this current GC. If not, they should record the object as encountered themselves,
    /// and enqueue the object reference to the object queue provided by the argument. If a binding moves objects, they should do the copying in the method,
    /// and enqueue the new object reference instead.
    ///
    /// The method should return the new object reference if the method moves the object, otherwise return the original object reference.
    ///
    /// Arguments:
    /// * `queue`: The object queue. If an object is encountered for the first time in this GC, we expect the implementation to call `queue.enqueue()`
    ///            for the object. If the object is moved during the tracing, the new object reference (after copying) should be enqueued instead.
    /// * `object`: The object to trace.
    /// * `worker`: The GC worker that is doing this tracing. This is used to copy object (see [`mmtk::vm::ObjectModel::copy`])
    fn vm_trace_object<Q: mmtk::ObjectQueue>(
        _queue: &mut Q,
        _object: mmtk::util::ObjectReference,
        _worker: &mut mmtk::scheduler::GCWorker<MMTKVMKit<Self>>,
    ) -> mmtk::util::ObjectReference {
        unimplemented!("VM does not implement vm_trace_object");
    }

    fn scan_roots(roots: impl RootsWorkFactory<Self::Slot>);
    fn post_forwarding() {}

    fn vmkit() -> &'static VMKit<Self>;
}

pub struct VMKit<R: Runtime> {
    pub mmtk: MMTK<MMTKVMKit<R>>,
    pub(crate) scanning: crate::mm::scanning::VMScanning<R>,
    pub(crate) threads: threads::Threads<R>,
    pub(crate) thunks: thunks::Thunks<R>,
}

unsafe impl<R: Runtime> Sync for VMKit<R> {}
unsafe impl<R: Runtime> Send for VMKit<R> {}

pub struct VMKitBuilder<R: Runtime> {
    pub mmtk_builder: MMTKBuilder,
    marker: PhantomData<R>,
}

impl<R> VMKitBuilder<R>
where
    R: Runtime,
{
    pub fn new() -> Self {
        Self {
            mmtk_builder: MMTKBuilder::new(),
            marker: PhantomData,
        }
    }

    pub fn from_options(mut self) -> Self {
        mmtk_options(&mut self.mmtk_builder).unwrap();
        self
    }

    pub fn build(self) -> VMKit<R> {
        VMKit {
            mmtk: self.mmtk_builder.build(),
            scanning: VMScanning::default(),
            threads: Threads::new(),
            thunks: Thunks::new(),
        }
    }
}

#[derive(Default)]
pub struct MMTKVMKit<R: Runtime>(R);

impl<R: Runtime> VMBinding for MMTKVMKit<R> {
    type VMObjectModel = crate::objectmodel::ObjectModel<R>;
    type VMScanning = crate::mm::scanning::VMScanning<R>;
    type VMActivePlan = crate::mm::active_plan::VMActivePlan<R>;
    type VMCollection = crate::mm::collection::VMCollection<R>;
    type VMMemorySlice = UnimplementedMemorySlice<R::Slot>;
    type VMReferenceGlue = UnimplementedRefGlue<R>;
    type VMSlot = R::Slot;
}

pub struct DisableGCScope;

static DISABLED_GC_SCOPE: AtomicUsize = AtomicUsize::new(0);

impl DisableGCScope {
    pub fn new() -> Self {
        DISABLED_GC_SCOPE.fetch_add(1, Ordering::AcqRel);
        Self
    }

    pub fn is_gc_disabled() -> bool {
        DISABLED_GC_SCOPE.load(Ordering::Acquire) != 0
    }
}

impl Drop for DisableGCScope {
    fn drop(&mut self) {
        DISABLED_GC_SCOPE.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Reference glue is not implemented. We have our own weak refs & finalizers processing.
pub struct UnimplementedRefGlue<R: Runtime>(PhantomData<R>);

impl<R: Runtime> ReferenceGlue<MMTKVMKit<R>> for UnimplementedRefGlue<R> {
    type FinalizableType = ObjectReference;
    fn clear_referent(_new_reference: mmtk::util::ObjectReference) {
        todo!()
    }
    fn enqueue_references(
        _references: &[mmtk::util::ObjectReference],
        _tls: mmtk::util::VMWorkerThread,
    ) {
        todo!()
    }

    fn get_referent(_object: mmtk::util::ObjectReference) -> Option<mmtk::util::ObjectReference> {
        todo!()
    }

    fn set_referent(_reff: mmtk::util::ObjectReference, _referent: mmtk::util::ObjectReference) {
        todo!()
    }
}
