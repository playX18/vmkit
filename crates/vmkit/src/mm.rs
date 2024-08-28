use crate::{
    mm::slot::SlotExt,
    objectmodel::{header::HeapObjectHeader, vtable::VTablePointer},
    runtime::threads::*,
    MMTKVMKit, Runtime, SlotOf, ThreadOf,
};
use mmtk::{
    util::{ObjectReference, VMMutatorThread},
    MutatorContext,
};

pub mod active_plan;
pub mod collection;
pub mod ptr_compr;
pub mod roots;
pub mod scanning;
pub mod shadow_stack;
pub mod slot;
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

#[inline]
pub extern "C" fn vmkit_allocate_los<R: Runtime>(
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
            mmtk::AllocationSemantics::Los,
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
    slot: *mut ObjectReference,
    target: Option<ObjectReference>,
) {
    let tls = ThreadOf::<R>::tls(thread.0);

    if tls.is_generational {
        let slot = SlotOf::<R>::from_pointer(slot);
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

/// Same as [`vmkit_write_barrier_post`] except fetches current thread on its own.
pub extern "C" fn vmkit_write_barrier_post2<R: Runtime>(
    src: ObjectReference,
    slot: *mut ObjectReference,
    target: Option<ObjectReference>,
) {
    let thread = vmkit_current_thread();
    let tls = ThreadOf::<R>::tls(thread);

    if tls.is_generational {
        let slot = SlotOf::<R>::from_pointer(slot);
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

/// A slow-path for write-barrier.
pub extern "C" fn vmkit_write_barrier_post_slow<R: Runtime>(
    src: ObjectReference,
    slot: *mut ObjectReference,
    target: Option<ObjectReference>,
) {
    let slot = SlotOf::<R>::from_pointer(slot);

    unsafe {
        let tls = vmkit_get_tls::<R>();
        if tls.is_generational {
            tls.mutator_mut_unchecked()
                .barrier()
                .object_reference_write_slow(src, slot, target);
        }
    }
}

#[inline(always)]
pub extern "C" fn vmkit_object_vtable<R: Runtime>(object: ObjectReference) -> VTablePointer {
    unsafe {
        let header = object
            .to_header::<MMTKVMKit<R>>()
            .as_ref::<HeapObjectHeader<R>>();

        header.vtable()
    }
}

#[inline(always)]
pub extern "C" fn vmkit_object_hash<R: Runtime>(object: ObjectReference) -> u64 {
    unsafe {
        let header = object
            .to_header::<MMTKVMKit<R>>()
            .as_ref::<HeapObjectHeader<R>>();

        header.hashcode()
    }
}
