//! # Object references
//!
//! Various types which are used to store object references.

use crate::{MMTKVMKit, Runtime};
use mmtk::util::{Address, ObjectReference};
use std::{
    marker::PhantomData,
    ptr::null_mut,
    sync::atomic::{AtomicPtr, Ordering},
};

/// The basic type from which all Member types are 'generated'.
///
/// Stores object pointer as atomic and can be null.
///
/// NOTE: We allow signals to be raised on null accesses, we handle them and throw proper panic instead (by default).
pub struct BasicMember<T, WeaknessTag> {
    pointer: AtomicPtr<T>,
    marker: PhantomData<WeaknessTag>,
}

impl<T, WeaknessTag> BasicMember<T, WeaknessTag> {
    pub fn slot<R: Runtime>(&self) -> R::Slot {
        <R::Slot as SlotExt>::from_member(self)
    }
    pub fn from_object_reference<R: Runtime>(objref: ObjectReference) -> Self {
        Self {
            pointer: AtomicPtr::new(objref.to_address::<MMTKVMKit<R>>().to_mut_ptr()),
            marker: PhantomData,
        }
    }

    pub fn from_address<R: Runtime>(address: Address) -> Self {
        Self {
            pointer: AtomicPtr::new(
                ObjectReference::from_raw_address(address)
                    .map(|addr| addr.to_raw_address().to_mut_ptr())
                    .unwrap_or(null_mut()),
            ),
            marker: PhantomData,
        }
    }

    pub fn from_raw_address(address: Address) -> Self {
        Self {
            pointer: AtomicPtr::new(address.to_mut_ptr()),
            marker: PhantomData,
        }
    }

    pub fn is_null(&self) -> bool {
        self.pointer.load(Ordering::Relaxed).is_null()
    }

    pub fn address(&self) -> Address {
        Address::from_ptr(self.pointer.load(Ordering::Relaxed))
    }

    pub fn object_reference<R: Runtime>(&self) -> Option<ObjectReference> {
        let addr = self.pointer.load(Ordering::Relaxed);

        if addr.is_null() {
            None
        } else {
            Some(ObjectReference::from_address::<MMTKVMKit<R>>(
                Address::from_ptr(addr),
            ))
        }
    }

    pub fn write(&self, objref: Option<ObjectReference>) {
        self.pointer.store(
            match objref {
                Some(o) => o.to_raw_address().to_mut_ptr(),
                None => null_mut(),
            },
            Ordering::Relaxed,
        );
    }
}

impl<U, OtherWeaknessTag, T, WeaknessTag> PartialEq<BasicMember<U, OtherWeaknessTag>>
    for BasicMember<T, WeaknessTag>
{
    fn eq(&self, other: &BasicMember<U, OtherWeaknessTag>) -> bool {
        self.pointer.load(Ordering::Relaxed) as usize
            == other.pointer.load(Ordering::Relaxed) as usize
    }
}

pub struct StrongMemberTag;
pub struct WeakMemberTag;
pub struct UntracedMemberTag;
///
/// Members are used in types to contain strong pointers to other garbage
/// collected objects. All Member fields of a type must be traced in the type's
/// trace method.
///
pub type Member<T> = BasicMember<T, StrongMemberTag>;

/// [`WeakMember`] is similar to Member in that it is used to point to other garbage
/// collected objects. However instead of creating a strong pointer to the
/// object, the WeakMember creates a weak pointer, which does not keep the
/// pointee alive. Hence if all pointers to to a heap allocated object are weak
/// the object will be garbage collected. At the time of GC the weak pointers
///  will automatically be set to null.
pub type WeakMember<T> = BasicMember<T, WeakMemberTag>;

/// [`UntracedMember`] is a pointer to an on-heap object that is not traced for some
/// reason. Do not use this unless you know what you are doing. Keeping raw
/// pointers to on-heap objects is prohibited unless used from stack. Pointee
/// must be kept alive through other means.
pub type UntracedMember<T> = BasicMember<T, UntracedMemberTag>;

pub trait SlotExt: Sized {
    fn from_member<T, Tag>(member: &BasicMember<T, Tag>) -> Self;
}
