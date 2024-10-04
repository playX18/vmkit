//! # Object references
//!
//! Various types which are used to store object references.

use crate::{mm::slot::SlotExt, Runtime};
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
pub struct BasicMember<'gc, T, WeaknessTag> {
    pointer: AtomicPtr<T>,
    marker: PhantomData<(&'gc T, WeaknessTag)>,
}

impl<'gc, T, WeaknessTag> BasicMember<'gc, T, WeaknessTag> {
    pub fn slot<R: Runtime>(&self) -> R::Slot {
        <R::Slot as SlotExt<R>>::from_member(self)
    }
    pub fn from_object_reference<R: Runtime>(objref: ObjectReference) -> Self {
        Self {
            pointer: AtomicPtr::new(objref.to_raw_address().to_mut_ptr()),
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
            unsafe {
                Some(ObjectReference::from_raw_address_unchecked(
                    Address::from_ptr(addr),
                ))
            }
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

impl<'gc, U, OtherWeaknessTag, T, WeaknessTag> PartialEq<BasicMember<'gc, U, OtherWeaknessTag>>
    for BasicMember<'gc, T, WeaknessTag>
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
pub type Member<'gc, T> = BasicMember<'gc, T, StrongMemberTag>;

/// [`WeakMember`] is similar to Member in that it is used to point to other garbage
/// collected objects. However instead of creating a strong pointer to the
/// object, the WeakMember creates a weak pointer, which does not keep the
/// pointee alive. Hence if all pointers to to a heap allocated object are weak
/// the object will be garbage collected. At the time of GC the weak pointers
///  will automatically be set to null.
pub type WeakMember<'gc, T> = BasicMember<'gc, T, WeakMemberTag>;

/// [`UntracedMember`] is a pointer to an on-heap object that is not traced for some
/// reason. Do not use this unless you know what you are doing. Keeping raw
/// pointers to on-heap objects is prohibited unless used from stack. Pointee
/// must be kept alive through other means.
pub type UntracedMember<'gc, T> = BasicMember<'gc, T, UntracedMemberTag>;

/// An managed reference to `T`, alias to [Member]. Only strong or untraced
/// references are allowed on-stack.
pub type Managed<'gc, T> = Member<'gc, T>;
pub type UntracedPtr<'gc, T> = UntracedMember<'gc, T>;
