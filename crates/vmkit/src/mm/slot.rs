use std::{hash::Hash, marker::PhantomData};

use crate::{objectmodel::vtable::*, MMTKVMKit};
use mmtk::{
    util::{Address, ObjectReference},
    vm::slot::Slot,
};

use crate::{
    objectmodel::{header::HeapObjectHeader, reference::BasicMember},
    Runtime, VTableOf,
};

pub trait SlotExt<R: Runtime>: Sized {
    fn from_member<T, Tag>(member: &BasicMember<T, Tag>) -> Self;
    fn from_pointer(pointer: *mut ObjectReference) -> Self;

    /// Construct a slot from VTableSlot. This function is invoked when `VTABLE_IS_OBJECT` is set to true,
    /// runtime can implement slot as an enum or use pointer tagging to store this effectively.
    fn from_vtable_slot(slot: VTableSlot<R>) -> Self {
        let _slot = slot;
        unimplemented!()
    }
}

pub struct VTableSlot<R: Runtime>(Address, PhantomData<R>);

impl<R: Runtime> VTableSlot<R> {
    pub fn new(objref: ObjectReference) -> Self {
        unsafe {
            Self(
                Address::from_ref(
                    objref
                        .to_header::<MMTKVMKit<R>>()
                        .as_ref::<HeapObjectHeader<R>>(),
                ),
                PhantomData,
            )
        }
    }

    pub const fn address(&self) -> Address {
        self.0
    }
}

impl<R: Runtime> Clone for VTableSlot<R> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<R: Runtime> Copy for VTableSlot<R> {}

impl<R: Runtime> PartialEq for VTableSlot<R> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<R: Runtime> Eq for VTableSlot<R> {}
impl<R: Runtime> Hash for VTableSlot<R> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl<R: Runtime> std::fmt::Debug for VTableSlot<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "VTableSlot({})", self.0)
    }
}

impl<R: Runtime> Slot for VTableSlot<R> {
    fn load(&self) -> Option<ObjectReference> {
        unsafe {
            let header = self.0.as_ref::<HeapObjectHeader<R>>();
            let vtable = header.vtable();

            VTableOf::<R>::to_object_reference(vtable)
        }
    }

    fn store(&self, object: ObjectReference) {
        unsafe {
            let header = self.0.as_mut_ref::<HeapObjectHeader<R>>();
            header.set_vtable(VTableOf::<R>::from_object_reference(object));
        }
    }
}
