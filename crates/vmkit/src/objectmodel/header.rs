use std::marker::PhantomData;

use easy_bitfield::{AtomicBitfieldContainer, BitField, BitFieldTrait, FromBitfield, ToBitfield};

pub type VTableBitfield = BitField<usize, VTablePointer, 0, 58, false>;
pub type HashStateBitfield = BitField<usize, HashState, { VTableBitfield::NEXT_BIT }, 2, false>;
pub type LocalLosMarkNurseryBitfield =
    BitField<usize, u8, { HashStateBitfield::NEXT_BIT }, 2, false>;

#[derive(Copy, Clone, Eq, PartialEq)]
#[repr(u8)]
pub enum HashState {
    Hashed = 0,
    HashedAndMoved,
    Unhashed,
}

use mmtk::util::ObjectReference;
use num_traits::{FromPrimitive, ToPrimitive};

use crate::{MMTKVMKit, Runtime};

use super::vtable::VTablePointer;

impl<S: FromPrimitive> ToBitfield<S> for HashState {
    fn one() -> Self {
        unreachable!()
    }

    fn zero() -> Self {
        unreachable!()
    }

    fn to_bitfield(self) -> S {
        S::from_u8(self as u8).unwrap()
    }
}

impl<S: ToPrimitive> FromBitfield<S> for HashState {
    fn from_bitfield(value: S) -> Self {
        let value = value.to_u8().unwrap();

        match value {
            0 => Self::Hashed,
            1 => Self::HashedAndMoved,
            2 => Self::Unhashed,
            _ => {
                #[cfg(debug_assertions)]
                {
                    unreachable!("invalid hash state")
                }

                #[cfg(not(debug_assertions))]
                unsafe {
                    std::hint::unreachable_unchecked();
                }
            }
        }
    }

    fn from_i64(_value: i64) -> Self {
        unreachable!()
    }
}

pub struct HeapObjectHeader<R: Runtime> {
    storage: AtomicBitfieldContainer<usize>,

    marker: PhantomData<R>,
}

impl<R: Runtime> HeapObjectHeader<R> {
    pub fn new(vtable: VTablePointer) -> Self {
        let this = Self {
            storage: AtomicBitfieldContainer::new(0),
            marker: PhantomData,
        };

        this.set_vtable(vtable);

        this
    }

    pub fn vtable(&self) -> VTablePointer {
        let vtable_ptr = self.storage.read::<VTableBitfield>();
        vtable_ptr
    }

    pub(crate) fn set_vtable(&self, vtable: VTablePointer) {
        self.storage.update_synchronized::<VTableBitfield>(vtable);
    }

    pub fn hash_state(&self) -> HashState {
        self.storage.read::<HashStateBitfield>()
    }

    pub(crate) fn set_hash_state(&self, state: HashState) {
        self.storage.update_synchronized::<HashStateBitfield>(state);
    }
}

impl<'a, R: Runtime> From<ObjectReference> for &'a HeapObjectHeader<R> {
    fn from(value: ObjectReference) -> Self {
        let value = value.to_header::<MMTKVMKit<R>>();
        unsafe { value.as_ref() }
    }
}
