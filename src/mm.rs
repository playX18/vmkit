/*
pub struct MemoryManager;

impl MemoryManager {
    /// Allocates object with `vtable` as vtable.
    ///
    /// Wrap into Member or any other smart-pointer manually.
    pub fn allocate<R: Runtime>(
        thread: &R::Thread,
        vtable: &'static VTable<R>,
        size: usize,
    ) -> ObjectReference {
        let tls = unsafe { &*thread.tls().get() };

        unsafe {
            let result = Address::from_usize(raw_align_up(
                tls.bump_top.load(Ordering::Relaxed),
                vtable.alignment.get(),
            ));

            if result + size <= Address::from_usize(tls.bump_end.load(Ordering::Relaxed)) {
                result.store(HeapObjectHeader::new(vtable));
                let objref = ObjectReference::from_address::<MMTKLibAlloc<R>>(result);

                return objref;
            } else {
                Self::allocate_slow(thread, vtable, size)
            }
        }
    }

    pub fn allocate_slow<R: Runtime>(
        thread: &R::Thread,
        vtable: &'static VTable<R>,
        size: usize,
    ) -> ObjectReference {
        let tls = unsafe { &mut *thread.tls().get() };
        tls.flush_bump_pointer();
        let mutator = unsafe { tls.mutator.assume_init_mut() };
        let semantics = if size > tls.los_threshold {
            AllocationSemantics::Los
        } else {
            AllocationSemantics::Default
        };
        let objref = if semantics == AllocationSemantics::Los {
            mmtk::memory_manager::alloc(
                &mut **mutator,
                size,
                vtable.alignment.get(),
                OBJECT_REF_OFFSET,
                AllocationSemantics::Los,
            )
        } else {
            mmtk::memory_manager::alloc_slow(
                mutator,
                size,
                vtable.alignment.get(),
                OBJECT_REF_OFFSET,
                AllocationSemantics::Default,
            )
        };

        tls.fetch_bump_pointer();
        unsafe { ObjectReference::from_raw_address_unchecked(objref) }
    }
}
*/
