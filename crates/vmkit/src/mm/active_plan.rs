use std::marker::PhantomData;

use mmtk::{vm::ActivePlan, Mutator};

use crate::{runtime::threads::Thread, MMTKVMKit, Runtime, ThreadOf};

pub struct VMActivePlan<R: Runtime>(PhantomData<R>);

impl<R: Runtime> ActivePlan<MMTKVMKit<R>> for VMActivePlan<R> {
    fn is_mutator(_tls: mmtk::util::VMThread) -> bool {
        true
    }

    fn mutator(tls: mmtk::util::VMMutatorThread) -> &'static mut mmtk::Mutator<MMTKVMKit<R>> {
        unsafe {
            let tls = ThreadOf::<R>::tls(tls.0);

            let mutator: *mut Box<Mutator<_>> = tls.mutator.as_ptr() as *mut _;
            &mut **mutator
        }
    }

    fn mutators<'a>() -> Box<dyn Iterator<Item = &'a mut mmtk::Mutator<MMTKVMKit<R>>> + 'a> {
        let threads = &R::vmkit().threads.threads.lock().unwrap();

        Box::new(
            threads
                .to_vec()
                .into_iter()
                .filter(|thread| ThreadOf::<R>::is_mutator(*thread))
                .map(|thread| unsafe {
                    let tls = ThreadOf::<R>::tls(thread);

                    let mutator = tls.mutator.as_ptr() as *mut Box<Mutator<_>>;

                    &mut **mutator
                }),
        )
    }

    fn number_of_mutators() -> usize {
        R::vmkit()
            .threads
            .threads
            .lock()
            .unwrap()
            .iter()
            .filter(|thread| ThreadOf::<R>::is_mutator(**thread))
            .count()
    }

    fn vm_trace_object<Q: mmtk::ObjectQueue>(
        _queue: &mut Q,
        _object: mmtk::util::ObjectReference,
        _worker: &mut mmtk::scheduler::GCWorker<MMTKVMKit<R>>,
    ) -> mmtk::util::ObjectReference {
        todo!()
    }
}
