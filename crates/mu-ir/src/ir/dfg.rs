use std::ops::{Index, IndexMut};

use cranelift_entity::{packed_option::ReservedValue, PrimaryMap, SecondaryMap};
use mu_utils::rc::P;
#[cfg(feature = "enable-serde")]
use serde_derive::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::types::Type;

use super::{
    entities::{Block, Inst, Value, ValueList, ValueListPool},
    instructions::{BlockCall, InstructionData},
};
#[derive(Clone, PartialEq, Hash, Debug)]
#[cfg_attr(feature = "enable-serde", derive(Serialize, Deserialize))]
pub struct Insts(PrimaryMap<Inst, InstructionData>);

/// Allow immutable access to instructions via indexing.
impl Index<Inst> for Insts {
    type Output = InstructionData;

    fn index(&self, inst: Inst) -> &InstructionData {
        self.0.index(inst)
    }
}

/// Allow mutable access to instructions via indexing.
impl IndexMut<Inst> for Insts {
    fn index_mut(&mut self, inst: Inst) -> &mut InstructionData {
        self.0.index_mut(inst)
    }
}

/// Storage for basic blocks within the DFG.
#[derive(Clone, PartialEq, Hash, Debug)]
#[cfg_attr(feature = "enable-serde", derive(Serialize, Deserialize))]
pub struct Blocks(PrimaryMap<Block, BlockData>);

impl Blocks {
    /// Create a new basic block.
    pub fn add(&mut self) -> Block {
        self.0.push(BlockData::new())
    }

    /// Get the total number of basic blocks created in this function, whether they are
    /// currently inserted in the layout or not.
    ///
    /// This is intended for use with `SecondaryMap::with_capacity`.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if the given block reference is valid.
    pub fn is_valid(&self, block: Block) -> bool {
        self.0.is_valid(block)
    }
}

impl Index<Block> for Blocks {
    type Output = BlockData;

    fn index(&self, block: Block) -> &BlockData {
        &self.0[block]
    }
}

impl IndexMut<Block> for Blocks {
    fn index_mut(&mut self, block: Block) -> &mut BlockData {
        &mut self.0[block]
    }
}

#[derive(Clone, Debug, PartialEq, Hash)]
#[cfg_attr(feature = "enable-serde", derive(Serialize, Deserialize))]
pub struct BlockData {
    params: ValueList,
}

impl BlockData {
    fn new() -> Self {
        Self {
            params: ValueList::new(),
        }
    }

    /// Get the parameters on `block`.
    pub fn params<'a>(&self, pool: &'a ValueListPool) -> &'a [Value] {
        self.params.as_slice(pool)
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "enable-serde", derive(Serialize, Deserialize))]
pub struct DataFlowGraph {
    pub insts: Insts,
    results: SecondaryMap<Inst, ValueList>,

    pub blocks: Blocks,
    pub value_lists: ValueListPool,
    values: PrimaryMap<Value, ValueData>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValueData {
    Inst { ty: P<Type>, num: u16, inst: Inst },
    Param { ty: P<Type>, num: u16, block: Block },
    Alias { ty: P<Type>, original: Value },
    Union { ty: P<Type>, x: Value, y: Value },
}

/// Where did a value come from?
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueDef {
    /// Value is the n'th result of an instruction.
    Result(Inst, usize),
    /// Value is the n'th parameter to a block.
    Param(Block, usize),
    /// Value is a union of two other values.
    Union(Value, Value),
}

impl ValueDef {
    /// Unwrap the instruction where the value was defined, or panic.
    pub fn unwrap_inst(&self) -> Inst {
        self.inst().expect("Value is not an instruction result")
    }

    /// Get the instruction where the value was defined, if any.
    pub fn inst(&self) -> Option<Inst> {
        match *self {
            Self::Result(inst, _) => Some(inst),
            _ => None,
        }
    }

    /// Unwrap the block there the parameter is defined, or panic.
    pub fn unwrap_block(&self) -> Block {
        match *self {
            Self::Param(block, _) => block,
            _ => panic!("Value is not a block parameter"),
        }
    }

    /// Get the number component of this definition.
    ///
    /// When multiple values are defined at the same program point, this indicates the index of
    /// this value.
    pub fn num(self) -> usize {
        match self {
            Self::Result(_, n) | Self::Param(_, n) => n,
            Self::Union(_, _) => 0,
        }
    }
}

impl DataFlowGraph {
    pub fn new() -> Self {
        Self {
            insts: Insts(PrimaryMap::new()),
            results: SecondaryMap::new(),
            blocks: Blocks(PrimaryMap::new()),
            value_lists: ValueListPool::new(),
            values: PrimaryMap::new(),
        }
    }

    pub fn clear(&mut self) {
        self.insts.0.clear();
        self.results.clear();
        self.blocks.0.clear();
        self.value_lists.clear();
        self.value_lists.clear();
    }

    pub fn num_insts(&self) -> usize {
        self.insts.0.len()
    }

    pub fn inst_is_valid(&self, inst: Inst) -> bool {
        self.insts.0.is_valid(inst)
    }

    pub fn num_blocks(&self) -> usize {
        self.blocks.len()
    }

    pub fn block_is_valid(&self, block: Block) -> bool {
        self.blocks.is_valid(block)
    }

    pub fn block_call(&mut self, block: Block, args: &[Value]) -> BlockCall {
        BlockCall::new(block, args, &mut self.value_lists)
    }

    pub fn num_values(&self) -> usize {
        self.values.len()
    }

    fn make_value(&mut self, data: ValueData) -> Value {
        self.values.push(data)
    }

    pub fn values<'a>(&'a self) -> Values {
        Values {
            inner: self.values.iter(),
        }
    }

    pub fn value_is_valid(&self, v: Value) -> bool {
        self.values.is_valid(v)
    }

    pub fn value_is_real(&self, v: Value) -> bool {
        self.value_is_valid(v) && !matches!(self.values[v], ValueData::Alias { .. })
    }

    pub fn value_def(&self, v: Value) -> ValueDef {
        match self.values[v] {
            ValueData::Inst { ty: _, num, inst } => ValueDef::Result(inst, num as _),
            ValueData::Param { ty: _, num, block } => ValueDef::Param(block, num as _),
            ValueData::Alias { ty: _, original } => self.value_def(self.resolve_aliases(original)),
            ValueData::Union { ty: _, x, y } => ValueDef::Union(x, y),
        }
    }

    pub fn resolve_aliases(&self, v: Value) -> Value {
        v
    }
}

pub struct Values<'a> {
    inner: cranelift_entity::Iter<'a, Value, ValueData>,
}

/// Check for non-values.
fn valid_valuedata(data: &ValueData) -> bool {
    if let ValueData::Alias { ty: _, original } = data {
        if original == &Value::reserved_value() {
            return false;
        }
    }
    true
}
impl<'a> Iterator for Values<'a> {
    type Item = Value;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner
            .by_ref()
            .find(|kv| valid_valuedata(kv.1))
            .map(|kv| kv.0)
    }
}
