use easy_bitfield::{FromBitfield, ToBitfield};
use mmtk::util::{Address, ObjectReference, OpaquePointer};
use num_traits::{FromPrimitive, ToPrimitive};

use crate::{
    mm::scanning::{Tracer, Visitor},
    Runtime,
};
use std::{mem::transmute, num::NonZeroUsize};

/// VTable representation for a runtime. This can be a pointer to vtable, index into vtable
/// storage, type-id or anything you can imagine. The main purpose of this trait is define a way to
/// get [`GCVTable`] for object.
///
/// You can also use object reference as a vtable, this can be useful in VMs like JVM where your vtable
/// is a class pointer which on its own most likely is allocated in GC heap. For this set [`VTABLE_IS_OBJECT`](`VTable::VTABLE_IS_OBJECT`) to true,
/// and imlement `from/to_object_reference` methods.
pub trait VTable<R: Runtime> {
    fn gc(&self) -> &GCVTable<R>;

    fn from_pointer<'a>(vtable: VTablePointer) -> &'a Self;
    fn to_pointer(&self) -> VTablePointer;

    /// Is VTable an object reference?
    ///
    /// NOTE: This constant when set to true disables slot enqueing of objects, this means all slots will instead be
    /// traced and updated in-place of their creation.
    const VTALBE_IS_OBJECT: bool = false;
    const ENQUEUE_VTABLE: bool = false;

    /// Get an object reference from corresponding vtable.
    ///
    /// Returns option because not in all cases vtable is an object e.g when you have immediate tags and
    /// resort to full-blown vtable only in special cases
    fn to_object_reference(_vtable: VTablePointer) -> Option<ObjectReference> {
        unimplemented!()
    }
    fn from_object_reference(_objref: ObjectReference) -> VTablePointer {
        unimplemented!()
    }
}
#[cfg(target_pointer_width = "64")]
pub const MAX_VTABLE_PTR: usize = 1 << 58;
#[cfg(target_pointer_width = "32")]
pub const MAX_VTABLE_PTR: usize = 1 << 30;
/// An opaque vtable pointer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(transparent)]
pub struct VTablePointer(pub OpaquePointer);

impl VTablePointer {
    pub fn new(address: Address) -> Option<Self> {
        let address_usize = address.as_usize();

        if address_usize > MAX_VTABLE_PTR {
            return None;
        }
        Some(Self(OpaquePointer::from_address(address)))
    }
}

/// The "metadata" for an allocated GC cell: the kind of cell, and
/// methods to "mark" (really, to invoke a GC callback on values in
/// the block) and (optionally) finalize the cell.
#[repr(C)]
pub struct GCVTable<R: Runtime> {
    /// This is used to ensure a VTable is valid and is not some other value.
    /// Notably, since VTable can sometimes be a forwarding pointer, if a pointer
    /// to a VTable is accidentally used as a ObjectReference, it will try to use the first
    /// word as another VTable.
    /// By putting in a magic number here, it will
    /// SIGSEGV on a specific address, which will make it easy to know exactly
    /// what went wrong.
    /// This is left on even in opt builds because VTable sizes are not
    /// particularly important.
    pub magic: u64,
    /// `size` should be the size of the cell if it is fixed, or 0 if it is
    /// variable sized.
    /// If it is variable sized, it should have `
    pub size: usize,
    pub alignment: NonZeroUsize,
    /// A callback to compute object size in case it's not static (e.g arrays or strings).
    pub compute_size: Option<extern "C" fn(*const ()) -> NonZeroUsize>,
    /// A callback to trace object fields.
    pub trace: TraceCallback<R>,
    pub finalize: FinalizeCallback,
}

impl<R: Runtime> VTable<R> for GCVTable<R> {
    fn gc(&self) -> &GCVTable<R> {
        self
    }

    fn from_pointer<'a>(vtable: VTablePointer) -> &'a Self {
        unsafe { transmute(vtable) }
    }

    fn to_pointer(&self) -> VTablePointer {
        unsafe { transmute(self) }
    }
}

pub enum TraceCallback<R: Runtime> {
    /// Object supports enqueing slots to fields (slot == reference to field).
    ScanSlots(fn(*mut (), &mut Visitor<R>)),
    /// Object can only scan fields directly.
    ScanObjects(fn(*mut (), &mut Tracer<R>)),
    /// Object does not require tracing
    NoTrace,
}

pub enum FinalizeCallback {
    Finalize(fn(*mut ())),
    Drop(fn(*mut ())),
    None,
}

impl<R: Runtime> GCVTable<R> {
    /// Value is 64 bits to make sure it can be used as a pointer in both 32 and
    /// 64-bit builds.
    /// "57ab1e" == "vtable".
    /// ff added at the beginning to make sure it's a kernel address (even in a
    /// 32-bit build).
    pub const MAGIC: u64 = 0xff57ab1eff57ab1e;

    pub fn size(&self) -> usize {
        self.size
    }
}

impl<S: FromPrimitive> ToBitfield<S> for VTablePointer {
    fn to_bitfield(self) -> S {
        S::from_usize(self.0.to_address().as_usize()).unwrap()
    }

    fn one() -> Self {
        unreachable!()
    }

    fn zero() -> Self {
        Self(OpaquePointer::from_address(unsafe { Address::zero() }))
    }
}

impl<S: ToPrimitive> FromBitfield<S> for VTablePointer {
    fn from_bitfield(value: S) -> Self {
        VTablePointer(OpaquePointer::from_address(unsafe {
            Address::from_usize(value.to_usize().unwrap())
        }))
    }

    fn from_i64(_value: i64) -> Self {
        unreachable!()
    }
}
