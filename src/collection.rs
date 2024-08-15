use std::{marker::PhantomData, mem::transmute};

use mmtk::{
    util::{OpaquePointer, VMThread, VMWorkerThread},
    vm::{ActivePlan, Collection, GCThreadContext},
};

use crate::{
    active_plan::VMActivePlan,
    threads::{self, GCBlockAdapter, Thread},
    DisableGCScope, MMTKLibAlloc, Runtime, ThreadOf,
};

pub struct VMCollection<R: Runtime>(PhantomData<R>);

impl<R: Runtime> Collection<MMTKLibAlloc<R>> for VMCollection<R> {
    fn block_for_gc(tls: mmtk::util::VMMutatorThread) {
        ThreadOf::<R>::block::<GCBlockAdapter<R>>(tls.0, false);
    }

    fn stop_all_mutators<F>(_tls: mmtk::util::VMWorkerThread, mut mutator_visitor: F)
    where
        F: FnMut(&'static mut mmtk::Mutator<MMTKLibAlloc<R>>),
    {
        threads::block_all_mutators_for_gc::<R>();
        let mutators = VMActivePlan::mutators();

        for mutator in mutators {
            mutator_visitor(mutator);
        }
    }

    fn is_collection_enabled() -> bool {
        DisableGCScope::is_gc_disabled()
    }

    fn out_of_memory(tls: mmtk::util::VMThread, err_kind: mmtk::util::alloc::AllocationError) {
        R::out_of_memory(tls, err_kind)
    }

    fn resume_mutators(_tls: mmtk::util::VMWorkerThread) {
        threads::unblock_all_mutators_for_gc::<R>();
    }

    fn vm_live_bytes() -> usize {
        R::vm_live_bytes()
    }

    fn post_forwarding(_tls: mmtk::util::VMWorkerThread) {}

    fn schedule_finalization(_tls: mmtk::util::VMWorkerThread) {}

    fn spawn_gc_thread(
        _tls: mmtk::util::VMThread,
        ctx: mmtk::vm::GCThreadContext<MMTKLibAlloc<R>>,
    ) {
        std::thread::spawn(move || match ctx {
            GCThreadContext::Worker(worker) => worker.run(
                VMWorkerThread(VMThread(OpaquePointer::from_address(unsafe {
                    transmute(R::current_thread())
                }))),
                R::mmtk_instance(),
            ),
        });
    }
}
