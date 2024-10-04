//! Mu type system.
//!
//!
//! This file stores all the types defined in Mu specification.

use std::{fmt, sync::LazyLock};

use cranelift_entity::{entity_impl, PrimaryMap};
use mu_utils::{rc::P, static_p};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    /// # `int<N>``
    ///
    /// Integer type, they are of variable bit-size and the maximum size is `u16::MAX`. Integers
    /// up to certain sizes are allocated inline, at the moment this is the case for integers where `N <= 128`
    Int(u16),
    /// # `float`
    ///
    /// Single-precision floating point type
    Float,
    /// # `double`
    ///
    /// Double-precision floating point type
    Double,

    /// # `uptr<T>`
    ///
    /// Native unsafe pointer to type `T`.
    UPtr(P<Type>),

    /// # `ufuncptr<SIG>`
    ///
    /// A native function pointer, all types of signature must be native-safe (no GC refs).
    UFuncPtr(P<Signature>),

    /// # `struct<T1,T2, ...>`
    ///
    /// Composite type which represents structures. The layout of `struct` is similar to that of C.
    Struct(StructId),

    /// # `hybrid<F1,F2,...V>`,
    ///
    /// Composite type which represents a fixed part and then a variable part of structure.
    Hybrid(HybridId),

    /// # `array<T length>`
    ///
    /// An array type, the size of it is statically known.
    Array(P<Type>, u64),
    /// # `vector<T length>`
    ///
    /// A scalar vector.
    Vector(P<Type>, u8),

    /// # `void`
    Void,

    /// # `ref<T>`
    ///
    /// Managed reference to type T
    Ref(P<Type>),

    /// # `iref<T>`
    ///
    /// Managed internal reference to type T
    IRef(P<Type>),

    /// # `weakref<T>`
    ///
    /// Managed weak reference to type T
    WeakRef(P<Type>),

    /// # `tagref64`
    ///
    /// A union type of double, int<52>, and `struct<ref<void> int<6>>`. It occupies 64 bits.
    TagRef64,

    /// # `funcref<SIG>`
    ///
    /// Function pointer to Mu function.
    FuncRef(P<Signature>),

    /// # `threadref`
    ///
    /// Reference to a Mu thread
    ThreadRef,
    /// # `stackref`
    ///
    /// Reference to a Mu stack
    StackRef,
}
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
pub struct Signature {
    pub arguments: Vec<P<Type>>,
    pub returns: Vec<P<Type>>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StructId(u32);

entity_impl!(StructId, "struct");

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HybridId(u32);

entity_impl!(HybridId, "hybrid");

static_p! {
    pub static ref ADDRESS_TYPE: Type = if cfg!(target_pointer_width="64") {
        Type::Int(64)
    } else {
        Type::Int(32)
    };

    pub static ref INT1_TYPE: Type = Type::Int(1);
    pub static ref INT8_TYPE: Type = Type::Int(8);
    pub static ref INT16_TYPE: Type = Type::Int(16);
    pub static ref INT32_TYPE: Type = Type::Int(32);
    pub static ref INT64_TYPE: Type = Type::Int(64);
    pub static ref INT128_TYPE: Type = Type::Int(128);
    pub static ref FLOAT_TYPE: Type = Type::Float;
    pub static ref DOUBLE_TYPE: Type = Type::Double;
    pub static ref VOID_TYPE: Type = Type::Void;
}

use parking_lot::{lock_api::MappedRwLockReadGuard, RawRwLock, RwLock, RwLockReadGuard};
use Type::*;

impl Type {
    pub const fn is_tagref64(&self) -> bool {
        matches!(self, TagRef64)
    }

    pub const fn is_stackref(&self) -> bool {
        matches!(self, StackRef)
    }

    pub const fn is_funcref(&self) -> bool {
        matches!(self, FuncRef(_))
    }

    pub const fn is_struct(&self) -> bool {
        matches!(self, Struct(_))
    }

    pub const fn is_void(&self) -> bool {
        matches!(self, Void)
    }

    pub const fn is_hybrid(&self) -> bool {
        matches!(self, Hybrid(_))
    }

    pub const fn is_int(&self) -> bool {
        matches!(self, Int(_))
    }

    pub const fn is_int_n(&self, width: u16) -> bool {
        matches!(self, Int(_width) if *_width == width)
    }

    pub const fn is_fp(&self) -> bool {
        matches!(self, Double | Float)
    }

    pub const fn is_opaque_ref(&self) -> bool {
        matches!(self, FuncRef(_) | StackRef | ThreadRef)
    }

    pub const fn is_eq_comparable(&self) -> bool {
        self.is_int() || self.is_ptr() || self.is_iref() || self.is_ref() || self.is_opaque_ref()
    }

    pub const fn is_double(&self) -> bool {
        matches!(self, Double)
    }

    pub const fn is_float(&self) -> bool {
        matches!(self, Float)
    }

    pub const fn is_scalar(&self) -> bool {
        matches!(
            self,
            Int(_)
                | Float
                | Double
                | Ref(_)
                | IRef(_)
                | WeakRef(_)
                | FuncRef(_)
                | UFuncPtr(_)
                | ThreadRef
                | StackRef
                | TagRef64
                | UPtr(_)
        )
    }

    pub const fn is_ref(&self) -> bool {
        matches!(self, Ref(_))
    }

    pub const fn is_iref(&self) -> bool {
        matches!(self, IRef(_))
    }

    pub const fn is_ptr(&self) -> bool {
        matches!(self, UPtr(_) | UFuncPtr(_))
    }

    pub const fn is_aggregate(&self) -> bool {
        matches!(self, Struct(_) | Hybrid(_) | Array(_, _))
    }

    pub fn is_traced(&self) -> bool {
        match self {
            Ref(_) | IRef(_) | WeakRef(_) | ThreadRef | StackRef | TagRef64 => true,
            Array(elem, _) | Self::Vector(elem, _) => elem.is_traced(),
            Hybrid(tag) => {
                let hybrid = tag.get();
                hybrid
                    .fields
                    .iter()
                    .map(|x| &**x)
                    .chain(std::iter::once(&*hybrid.var))
                    .any(Type::is_traced)
            }

            Struct(tag) => {
                let struct_ = tag.get();

                struct_.fields.iter().map(|x| &**x).any(Type::is_traced)
            }
            _ => false,
        }
    }

    pub fn is_native(&self) -> bool {
        match self {
            Self::Int(_) | Self::Float | Self::Double | Self::Void => true,
            Self::Array(elem, _) | Self::Vector(elem, _) => elem.is_native(),
            Self::UPtr(_) | Self::UFuncPtr(_) => true,
            Self::Hybrid(tag) => {
                let hybrid = tag.get();
                hybrid
                    .fields
                    .iter()
                    .map(|x| &**x)
                    .chain(std::iter::once(&*hybrid.var))
                    .any(Type::is_native)
            }

            Struct(tag) => {
                let struct_ = tag.get();

                struct_.fields.iter().map(|x| &**x).any(Type::is_native)
            }
            _ => false,
        }
    }

    pub fn get_elem_type(&self) -> Option<&P<Type>> {
        match self {
            Array(elem, _) | Vector(elem, _) => Some(elem),
            _ => None,
        }
    }

    pub fn get_hybrid_var(&self) -> Option<P<Type>> {
        match self {
            Hybrid(tag) => Some(tag.get().var.clone()),
            _ => None,
        }
    }

    pub fn get_referent(&self) -> Option<P<Type>> {
        match self {
            Ref(ty) | IRef(ty) | WeakRef(ty) | UPtr(ty) => Some(ty.clone()),
            _ => None,
        }
    }

    pub fn get_signature(&self) -> Option<P<Signature>> {
        match self {
            FuncRef(sig) | UFuncPtr(sig) => Some(sig.clone()),
            _ => None,
        }
    }

    pub fn get_int_width(&self) -> Option<usize> {
        match self {
            Int(len) => Some(*len as usize),
            Ref(_) | IRef(_) | WeakRef(_) | UPtr(_) | ThreadRef | StackRef | TagRef64
            | FuncRef(_) | UFuncPtr(_) => Some(0usize.leading_zeros() as usize),
            _ => None,
        }
    }

    pub fn struct_empty() -> StructId {
        let mut map = COMPOSITE_TYPES.structures.write();
        map.push(StructureType { fields: vec![] })
    }

    pub fn struct_put(tag: StructId, fields: &[P<Type>]) {
        let mut map = COMPOSITE_TYPES.structures.write();

        map.get_mut(tag).unwrap().fields = fields.to_vec();
    }

    pub fn r#struct(fields: &[P<Type>]) -> StructId {
        let mut map = COMPOSITE_TYPES.structures.write();

        map.push(StructureType {
            fields: fields.to_vec(),
        })
    }

    pub fn hybrid_empty() -> HybridId {
        let mut map = COMPOSITE_TYPES.hybrids.write();

        map.push(HybridType {
            fields: vec![],
            var: VOID_TYPE.clone(),
        })
    }

    pub fn hybrid_put(tag: HybridId, fields: &[P<Type>], var: P<Type>) {
        let mut map = COMPOSITE_TYPES.hybrids.write();

        let h = map.get_mut(tag).unwrap();

        h.fields = fields.to_vec();
        h.var = var;
    }

    pub fn hybrid(fields: &[P<Type>], var: P<Type>) -> HybridId {
        let mut map = COMPOSITE_TYPES.hybrids.write();

        map.push(HybridType {
            fields: fields.to_vec(),
            var,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructureType {
    pub fields: Vec<P<Type>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HybridType {
    pub fields: Vec<P<Type>>,
    pub var: P<Type>,
}

pub struct CompositeTypeStorage {
    pub structures: RwLock<PrimaryMap<StructId, StructureType>>,
    pub hybrids: RwLock<PrimaryMap<HybridId, HybridType>>,
}

pub static COMPOSITE_TYPES: LazyLock<CompositeTypeStorage> =
    LazyLock::new(|| CompositeTypeStorage {
        hybrids: RwLock::new(PrimaryMap::new()),
        structures: RwLock::new(PrimaryMap::new()),
    });

impl HybridId {
    pub fn get<'a>(self) -> MappedRwLockReadGuard<'a, RawRwLock, HybridType> {
        RwLockReadGuard::map(COMPOSITE_TYPES.hybrids.read(), |map| map.get(self).unwrap())
    }
}

impl StructId {
    pub fn get<'a>(self) -> MappedRwLockReadGuard<'a, RawRwLock, StructureType> {
        RwLockReadGuard::map(COMPOSITE_TYPES.structures.read(), |map| {
            map.get(self).unwrap()
        })
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Int(n) => write!(f, "int<{}>", n),
            Float => write!(f, "float"),
            Double => write!(f, "double"),
            Ref(ty) => write!(f, "ref<{}>", ty),
            IRef(ty) => write!(f, "iref<{}>", ty),
            WeakRef(ty) => write!(f, "weakref<{}>", ty),
            UPtr(ty) => write!(f, "uptr<{}>", ty),
            Array(ty, len) => write!(f, "array<{} {}>", ty, len),
            Void => write!(f, "void"),
            ThreadRef => write!(f, "threadref"),
            StackRef => write!(f, "stackref"),
            TagRef64 => write!(f, "tagref64"),
            Vector(ty, size) => write!(f, "vector<{} {}>", ty, size),
            FuncRef(sig) => write!(f, "funcref<{}>", sig),
            UFuncPtr(sig) => write!(f, "ufuncptr<{}>", sig),
            Struct(s) => write!(f, "{}", s),
            Hybrid(h) => write!(f, "{}", h),
        }
    }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let args = self
            .arguments
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let returns = self
            .returns
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");

        write!(f, "({})->({})", args, returns)
    }
}
