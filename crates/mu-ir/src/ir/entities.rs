use cranelift_entity::entity_impl;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct FunctionVersionId(u32);

entity_impl!(FunctionVersionId, "function_version");

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct FunctionId(u32);

entity_impl!(FunctionId, "function");

use std::fmt::{Display, Formatter};

/// An opaque reference to a [basic block](https://en.wikipedia.org/wiki/Basic_block) in a
/// [`Function`](super::function::Function).
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "enable-serde", derive(Serialize, Deserialize))]
pub struct Block(u32);
entity_impl!(Block, "block");

impl Block {
    /// Create a new block reference from its number. This corresponds to the `blockNN` representation.
    ///
    /// This method is for use by the parser.
    pub fn with_number(n: u32) -> Option<Self> {
        if n < u32::MAX {
            Some(Self(n))
        } else {
            None
        }
    }
}

/// Some instructions use an external list of argument values because there is not enough space in
/// the 16-byte `InstructionData` struct. These value lists are stored in a memory pool in
/// `dfg.value_lists`.
pub type ValueList = cranelift_entity::EntityList<Value>;

/// Memory pool for holding value lists. See `ValueList`.
pub type ValueListPool = cranelift_entity::ListPool<Value>;

/// An opaque reference to an SSA value.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "enable-serde", derive(Serialize, Deserialize))]
pub struct Value(u32);
entity_impl!(Value, "v");

impl Value {
    /// Create a value from its number representation.
    /// This is the number in the `vNN` notation.
    ///
    /// This method is for use by the parser.
    pub fn with_number(n: u32) -> Option<Self> {
        if n < u32::MAX / 2 {
            Some(Self(n))
        } else {
            None
        }
    }
}

/// An opaque reference to an instruction in a [`Function`](super::Function).
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "enable-serde", derive(Serialize, Deserialize))]
pub struct Inst(u32);
entity_impl!(Inst, "inst");

#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "enable-serde", derive(Serialize, Deserialize))]
pub struct GlobalValue(u32);
entity_impl!(GlobalValue, "gv");

impl GlobalValue {
    /// Create a new global value reference from its number.
    ///
    /// This method is for use by the parser.
    pub fn with_number(n: u32) -> Option<Self> {
        if n < u32::MAX {
            Some(Self(n))
        } else {
            None
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
#[cfg_attr(feature = "enable-serde", derive(Serialize, Deserialize))]
pub struct Constant(u32);
entity_impl!(Constant, "const");

impl Constant {
    /// Create a const reference from its number.
    ///
    /// This method is for use by the parser.
    pub fn with_number(n: u32) -> Option<Self> {
        if n < u32::MAX {
            Some(Self(n))
        } else {
            None
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "enable-serde", derive(Serialize, Deserialize))]
pub struct Immediate(u32);
entity_impl!(Immediate, "imm");

impl Immediate {
    /// Create an immediate reference from its number.
    ///
    /// This method is for use by the parser.
    pub fn with_number(n: u32) -> Option<Self> {
        if n < u32::MAX {
            Some(Self(n))
        } else {
            None
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "enable-serde", derive(Serialize, Deserialize))]
pub struct JumpTable(u32);
entity_impl!(JumpTable, "jt");

impl JumpTable {
    /// Create a new jump table reference from its number.
    ///
    /// This method is for use by the parser.
    pub fn with_number(n: u32) -> Option<Self> {
        if n < u32::MAX {
            Some(Self(n))
        } else {
            None
        }
    }
}
