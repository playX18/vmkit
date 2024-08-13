use std::{marker::PhantomData, mem::transmute, sync::atomic::Ordering};

use mmtk::{
    util::{OpaquePointer, VMMutatorThread, VMThread, VMWorkerThread},
    vm::{ActivePlan, Collection, GCThreadContext},
};

use crate::{
    active_plan::VMActivePlan,
    threads::{GCBlockAdapter, Thread, ThreadState},
    DisableGCScope, MMTKLibAlloc, Runtime,
};

pub struct VMCollection<R: Runtime>(PhantomData<R>);

impl<R: Runtime> Collection<MMTKLibAlloc<R>> for VMCollection<R> {
    fn block_for_gc(tls: mmtk::util::VMMutatorThread) {
        let thread = <R::Thread as Thread<R>>::from_vm_mutator_thread(tls);
        thread.block::<GCBlockAdapter<R>>(false);
    }

    fn stop_all_mutators<F>(_tls: mmtk::util::VMWorkerThread, mut mutator_visitor: F)
    where
        F: FnMut(&'static mut mmtk::Mutator<MMTKLibAlloc<R>>),
    {
        println!("Stopping all mutators");

        let mutators = VMActivePlan::mutators();

        for mutator in mutators {
            mutator_visitor(mutator);
        }
    }

    fn is_collection_enabled() -> bool {
        DisableGCScope::is_gc_disabled()
    }

    fn out_of_memory(tls: mmtk::util::VMThread, err_kind: mmtk::util::alloc::AllocationError) {
        R::out_of_memory(
            <R::Thread as Thread<R>>::from_vm_mutator_thread(VMMutatorThread(tls)),
            err_kind,
        )
    }

    fn resume_mutators(_tls: mmtk::util::VMWorkerThread) {}

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
