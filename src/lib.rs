use std::{
    marker::PhantomData,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use mmtk::{
    util::{alloc::AllocationError, ObjectReference},
    vm::{
        slot::{Slot, UnimplementedMemorySlice},
        ReferenceGlue, VMBinding,
    },
    MMTK,
};
use objectmodel::reference::SlotExt;
pub mod active_plan;
pub mod collection;
pub mod mm;
pub mod mock;
pub mod objectmodel;
pub mod safepoint;
pub mod scanning;
pub mod shadow_stack;
pub mod sync;
pub mod threads;

pub type ThreadOf<R> = <R as Runtime>::Thread;
pub type SlotOf<R> = <R as Runtime>::Slot;
pub trait Runtime: 'static + Default + Send + Sync {
    type Slot: Slot + SlotExt;
    type Thread: threads::Thread<Self>;

    fn try_current_thread() -> Option<Arc<Self::Thread>>;
    fn current_thread() -> Arc<Self::Thread>;

    fn threads() -> &'static threads::Threads<Self>;

    fn out_of_memory(thread: &'static Self::Thread, error: AllocationError);
    fn vm_live_bytes() -> usize {
        0
    }

    fn mmtk_instance() -> &'static MMTK<MMTKLibAlloc<Self>>;
}

#[derive(Default)]
pub struct MMTKLibAlloc<R: Runtime>(R);

impl<R: Runtime> VMBinding for MMTKLibAlloc<R> {
    type VMObjectModel = objectmodel::ObjectModel<R>;
    type VMScanning = scanning::VMScanning;
    type VMActivePlan = active_plan::VMActivePlan<R>;
    type VMCollection = collection::VMCollection<R>;
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

impl<R: Runtime> ReferenceGlue<MMTKLibAlloc<R>> for UnimplementedRefGlue<R> {
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
