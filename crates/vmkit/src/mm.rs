use crate::{
    objectmodel::{header::HeapObjectHeader, vtable::VTablePointer},
    threads::*,
    Runtime, ThreadOf,
};
use mmtk::util::{ObjectReference, VMMutatorThread};

pub mod active_plan;
pub mod collection;
pub mod roots;
pub mod scanning;
pub mod shadow_stack;
pub mod tlab;

#[inline]
pub extern "C" fn vmkit_allocate<R: Runtime>(
    thread: VMMutatorThread,
    size: usize,
    vtable: VTablePointer,
) -> ObjectReference {
    let tls = ThreadOf::<R>::tls(thread.0);

    unsafe {
        let tlab = tls.tlab_mut_unchecked();
        let mmtk_mutator = tls.mutator_mut_unchecked();

        let mut result = tlab.allocate(mmtk_mutator, size, align_of::<usize>() * 2);

        result.store(HeapObjectHeader::<R>::new(vtable));
        result += size_of::<HeapObjectHeader<R>>();

        ObjectReference::from_raw_address_unchecked(result)
    }
}

#[inline]
pub extern "C" fn vmkit_allocate_immortal<R: Runtime>(
    thread: VMMutatorThread,
    size: usize,
    vtable: VTablePointer,
) -> ObjectReference {
    let tls = ThreadOf::<R>::tls(thread.0);
    unsafe {
        let tlab = tls.tlab_mut_unchecked();
        let mmtk_mutator = tls.mutator_mut_unchecked();
        tlab.flush_cursors(mmtk_mutator);
        let mut result = mmtk::memory_manager::alloc(
            mmtk_mutator,
            size,
            align_of::<usize>() * 2,
            0,
            mmtk::AllocationSemantics::Immortal,
        );
        tlab.bump_cursors(mmtk_mutator);

        result.store(HeapObjectHeader::<R>::new(vtable));
        result += size_of::<HeapObjectHeader<R>>();

        ObjectReference::from_raw_address_unchecked(result)
    }
}

#[inline]
pub extern "C" fn vmkit_allocate_nonmoving<R: Runtime>(
    thread: VMMutatorThread,
    size: usize,
    vtable: VTablePointer,
) -> ObjectReference {
    let tls = ThreadOf::<R>::tls(thread.0);
    unsafe {
        let tlab = tls.tlab_mut_unchecked();
        let mmtk_mutator = tls.mutator_mut_unchecked();
        tlab.flush_cursors(mmtk_mutator);
        let mut result = mmtk::memory_manager::alloc(
            mmtk_mutator,
            size,
            align_of::<usize>() * 2,
            0,
            mmtk::AllocationSemantics::NonMoving,
        );
        tlab.bump_cursors(mmtk_mutator);
        result.store(HeapObjectHeader::<R>::new(vtable));
        result += size_of::<HeapObjectHeader<R>>();

        ObjectReference::from_raw_address_unchecked(result)
    }
}

pub extern "C" fn vmkit_write_barrier_post<R: Runtime>(
    thread: VMMutatorThread,
    src: ObjectReference,
    slot: R::Slot,
    target: Option<ObjectReference>,
) {
    let tls = ThreadOf::<R>::tls(thread.0);

    if tls.is_generational {
        unsafe {
            mmtk::memory_manager::object_reference_write_post(
                tls.mutator_mut_unchecked(),
                src,
                slot,
                target,
            )
        }
    }
}
