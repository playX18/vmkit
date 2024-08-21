use std::{
    marker::PhantomData,
    sync::atomic::{AtomicUsize, Ordering},
};

use mm::scanning::VMScanning;
use mmtk::{
    util::{alloc::AllocationError, ObjectReference, VMThread},
    vm::{
        slot::{Slot, UnimplementedMemorySlice},
        ReferenceGlue, RootsWorkFactory, VMBinding,
    },
    MMTKBuilder, MMTK,
};
use objectmodel::{reference::SlotExt, vtable::VTable};

pub use mmtk;
use runtime::threads::Threads;
use runtime::thunks::Thunks;

pub mod arch;
pub mod compiler;
pub mod mm;
pub mod objectmodel;
pub mod runtime;
pub mod sync;

pub type ThreadOf<R> = <R as Runtime>::Thread;
pub type SlotOf<R> = <R as Runtime>::Slot;
pub type VTableOf<R> = <R as Runtime>::VTable;
pub trait Runtime: 'static + Default + Send + Sync {
    type Slot: Slot + SlotExt;
    type VTable: VTable<Self>;
    type Thread: runtime::threads::Thread<Self>;

    /// An accessor for thread-local storage of current thread. You can simply use `thread_local!` and return
    /// pointer to it.
    fn current_thread() -> VMThread {
        runtime::threads::vmkit_current_thread()
    }
    fn out_of_memory(thread: VMThread, error: AllocationError);
    fn vm_live_bytes() -> usize {
        0
    }

    fn scan_roots(roots: impl RootsWorkFactory<Self::Slot>);
    fn post_forwarding() {}

    fn vmkit() -> &'static VMKit<Self>;
}

pub struct VMKit<R: Runtime> {
    pub mmtk: MMTK<MMTKVMKit<R>>,
    pub(crate) scanning: mm::scanning::VMScanning<R>,
    pub(crate) threads: runtime::threads::Threads<R>,
    pub(crate) thunks: Thunks<R>,
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
    type VMObjectModel = objectmodel::ObjectModel<R>;
    type VMScanning = mm::scanning::VMScanning<R>;
    type VMActivePlan = mm::active_plan::VMActivePlan<R>;
    type VMCollection = mm::collection::VMCollection<R>;
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
