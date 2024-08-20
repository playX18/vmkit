//! # Thread-local Allocation Buffer
//!
//! A type which allows to perform fast allocations out of the MMTk Mutator. Avoids indirection
//! and expensive computation caused by determining allocator.

use std::{marker::PhantomData, mem::offset_of};

use mmtk::{
    util::{
        alloc::{AllocatorSelector, BumpAllocator, ImmixAllocator},
        Address,
    },
    Mutator,
};

use crate::{MMTKVMKit, Runtime};

#[repr(C)]
pub struct TLAB<R: Runtime> {
    bump_cursor: Address,
    bump_end: Address,
    selector: AllocatorSelector,
    los_threshold: usize,
    marker: PhantomData<R>,
}

impl<R: Runtime> TLAB<R> {
    pub const CURSOR_OFFSET: usize = offset_of!(Self, bump_cursor);
    pub const END_OFFSET: usize = offset_of!(Self, bump_end);
    pub const LOS_THRESHOLD_OFFSET: usize = offset_of!(Self, los_threshold);

    pub fn new() -> Self {
        let selector = mmtk::memory_manager::get_allocator_mapping(
            &R::vmkit().mmtk,
            mmtk::AllocationSemantics::Default,
        );

        let los_threshold = R::vmkit()
            .mmtk
            .get_plan()
            .constraints()
            .max_non_los_default_alloc_bytes;

        Self {
            bump_cursor: Address::ZERO,
            bump_end: Address::ZERO,
            los_threshold,
            selector,
            marker: PhantomData,
        }
    }

    pub fn allocate(
        &mut self,
        mutator: &mut Mutator<MMTKVMKit<R>>,
        size: usize,
        align: usize,
    ) -> Address {
        let mut result = self.bump_cursor - size;
        result = result.align_down(align);
        if result < self.bump_end {
            return self.allocate_slow(mutator, size, align);
        }

        self.bump_cursor = result;

        result
    }

    pub fn allocate_slow(
        &mut self,
        mutator: &mut Mutator<MMTKVMKit<R>>,
        size: usize,
        align: usize,
    ) -> Address {
        unsafe {
            self.flush_cursors(mutator);
        }
        let addr = if size >= self.los_threshold {
            mmtk::memory_manager::alloc(mutator, size, align, 0, mmtk::AllocationSemantics::Los)
        } else {
            mmtk::memory_manager::alloc_slow(
                mutator,
                size,
                align,
                0,
                mmtk::AllocationSemantics::Default,
            )
        };

        unsafe {
            self.bump_cursors(mutator);
        }
        addr
    }

    pub unsafe fn flush_cursors(&mut self, mutator: &mut Mutator<MMTKVMKit<R>>) {
        let bump_pointer = unsafe {
            let selector = self.selector;

            match selector {
                AllocatorSelector::BumpPointer(_) => {
                    &mut mutator
                        .allocator_impl_mut::<BumpAllocator<MMTKVMKit<R>>>(selector)
                        .bump_pointer
                }

                AllocatorSelector::Immix(_) => {
                    &mut mutator
                        .allocator_impl_mut::<ImmixAllocator<MMTKVMKit<R>>>(selector)
                        .bump_pointer
                }

                _ => {
                    return;
                }
            }
        };

        // we bump downwards so start is bump_end and end is bump_cursor
        bump_pointer.reset(self.bump_end, self.bump_cursor);
    }

    pub unsafe fn bump_cursors(&mut self, mutator: &mut Mutator<MMTKVMKit<R>>) {
        let bump_pointer = unsafe {
            let selector = self.selector;

            match selector {
                AllocatorSelector::BumpPointer(_) => {
                    &mutator
                        .allocator_impl::<BumpAllocator<MMTKVMKit<R>>>(selector)
                        .bump_pointer
                }

                AllocatorSelector::Immix(_) => {
                    &mutator
                        .allocator_impl::<ImmixAllocator<MMTKVMKit<R>>>(selector)
                        .bump_pointer
                }

                _ => {
                    return;
                }
            }
        };

        self.bump_end = bump_pointer.cursor;
        self.bump_cursor = bump_pointer.limit;
    }
}
