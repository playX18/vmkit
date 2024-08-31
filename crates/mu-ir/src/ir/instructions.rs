use std::fmt::{Display, Formatter};

use mu_utils::rc::P;

use crate::types::Type;

use super::entities::*;

// A pair of a Block and its arguments, stored in a single EntityList internally.
///
/// NOTE: We don't expose either value_to_block or block_to_value outside of this module because
/// this operation is not generally safe. However, as the two share the same underlying layout,
/// they can be stored in the same value pool.
///
/// BlockCall makes use of this shared layout by storing all of its contents (a block and its
/// argument) in a single EntityList. This is a bit better than introducing a new entity type for
/// the pair of a block name and the arguments entity list, as we don't pay any indirection penalty
/// to get to the argument values -- they're stored in-line with the block in the same list.
///
/// The BlockCall::new function guarantees this layout by requiring a block argument that's written
/// in as the first element of the EntityList. Any subsequent entries are always assumed to be real
/// Values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "enable-serde", derive(Serialize, Deserialize))]
pub struct BlockCall {
    /// The underlying storage for the BlockCall. The first element of the values EntityList is
    /// guaranteed to always be a Block encoded as a Value via BlockCall::block_to_value.
    /// Consequently, the values entity list is never empty.
    values: cranelift_entity::EntityList<Value>,
}

impl BlockCall {
    // NOTE: the only uses of this function should be internal to BlockCall. See the block comment
    // on BlockCall for more context.
    fn value_to_block(val: Value) -> Block {
        Block::from_u32(val.as_u32())
    }

    // NOTE: the only uses of this function should be internal to BlockCall. See the block comment
    // on BlockCall for more context.
    fn block_to_value(block: Block) -> Value {
        Value::from_u32(block.as_u32())
    }

    /// Construct a BlockCall with the given block and arguments.
    pub fn new(block: Block, args: &[Value], pool: &mut ValueListPool) -> Self {
        let mut values = ValueList::default();
        values.push(Self::block_to_value(block), pool);
        values.extend(args.iter().copied(), pool);
        Self { values }
    }

    /// Return the block for this BlockCall.
    pub fn block(&self, pool: &ValueListPool) -> Block {
        let val = self.values.first(pool).unwrap();
        Self::value_to_block(val)
    }

    /// Replace the block for this BlockCall.
    pub fn set_block(&mut self, block: Block, pool: &mut ValueListPool) {
        *self.values.get_mut(0, pool).unwrap() = Self::block_to_value(block);
    }

    /// Append an argument to the block args.
    pub fn append_argument(&mut self, arg: Value, pool: &mut ValueListPool) {
        self.values.push(arg, pool);
    }

    /// Return a slice for the arguments of this block.
    pub fn args_slice<'a>(&self, pool: &'a ValueListPool) -> &'a [Value] {
        &self.values.as_slice(pool)[1..]
    }

    /// Return a slice for the arguments of this block.
    pub fn args_slice_mut<'a>(&'a mut self, pool: &'a mut ValueListPool) -> &'a mut [Value] {
        &mut self.values.as_mut_slice(pool)[1..]
    }

    /// Remove the argument at ix from the argument list.
    pub fn remove(&mut self, ix: usize, pool: &mut ValueListPool) {
        self.values.remove(1 + ix, pool)
    }

    /// Clear out the arguments list.
    pub fn clear(&mut self, pool: &mut ValueListPool) {
        self.values.truncate(1, pool)
    }

    /// Appends multiple elements to the arguments.
    pub fn extend<I>(&mut self, elements: I, pool: &mut ValueListPool)
    where
        I: IntoIterator<Item = Value>,
    {
        self.values.extend(elements, pool)
    }

    /// Return a value that can display this block call.
    pub fn display<'a>(&self, pool: &'a ValueListPool) -> DisplayBlockCall<'a> {
        DisplayBlockCall { block: *self, pool }
    }

    /// Deep-clone the underlying list in the same pool. The returned
    /// list will have identical contents but changes to this list
    /// will not change its contents or vice-versa.
    pub fn deep_clone(&self, pool: &mut ValueListPool) -> Self {
        Self {
            values: self.values.deep_clone(pool),
        }
    }
}

/// Wrapper for the context needed to display a [BlockCall] value.
pub struct DisplayBlockCall<'a> {
    block: BlockCall,
    pool: &'a ValueListPool,
}

impl<'a> Display for DisplayBlockCall<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.block.block(&self.pool))?;
        let args = self.block.args_slice(&self.pool);
        if !args.is_empty() {
            write!(f, "(")?;
            for (ix, arg) in args.iter().enumerate() {
                if ix > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{arg}")?;
            }
            write!(f, ")")?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum BinaryOpcode {
    // BinOp Int(n) Int(n) -> Int(n)
    Add,
    Sub,
    Mul,
    Sdiv,
    Srem,
    Udiv,
    Urem,
    And,
    Or,
    Xor,

    // BinOp Int(n) Int(m) -> Int(n)
    Shl,
    Lshr,
    Ashr,

    // BinOp FP FP -> FP
    FAdd,
    FSub,
    FMul,
    FDiv,
    FRem,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AtomicOpcode {
    XCHG,
    ADD,
    SUB,
    AND,
    NAND,
    OR,
    XOR,
    MAX,
    MIN,
    UMAX,
    UMIN,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CompareOpcode {
    // for Int comparison
    EQ,
    NE,
    SGE,
    SGT,
    SLE,
    SLT,
    UGE,
    UGT,
    ULE,
    ULT,

    // for FP comparison
    FFALSE,
    FTRUE,
    FOEQ,
    FOGT,
    FOGE,
    FOLT,
    FOLE,
    FONE,
    FORD,
    FUEQ,
    FUGT,
    FUGE,
    FULT,
    FULE,
    FUNE,
    FUNO,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConvOpcode {
    TRUNC,
    ZEXT,
    SEXT,
    FPTRUNC,
    FPEXT,
    FPTOUI,
    FPTOSI,
    UITOFP,
    SITOFP,
    BITCAST,
    REFCAST,
    PTRCAST,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResumptionData {
    pub normal: BlockCall,
    pub exception: BlockCall,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CallData {
    pub func: Value,
    pub args: ValueList,
}

#[derive(PartialEq, Eq, Copy, Clone, Debug, Hash)]
pub enum MemoryOrder {
    NotAtomic,
    Relaxed,
    Consume,
    Acquire,
    Release,
    AcqRel,
    SeqCst,
}

/// BinOpStatus represents status flags from a binary operation
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BinOpStatus {
    /// negative flag
    pub flag_n: bool,
    /// zero flag
    pub flag_z: bool,
    /// carry flag
    pub flag_c: bool,
    /// overflow flag
    pub flag_v: bool,
}

#[derive(Clone, Debug, PartialEq, Hash)]
pub enum InstructionData {
    Binary(BinaryOpcode, [Value; 2], Option<ResumptionData>),
    BinaryWithStats(
        BinaryOpcode,
        BinOpStatus,
        [Value; 2],
        Option<ResumptionData>,
    ),
    Compare(CompareOpcode, [Value; 2], Option<ResumptionData>),

    Conv {
        opcode: ConvOpcode,
        from: P<Type>,
        to: P<Type>,
        arg: Value,
        resume: Option<ResumptionData>,
    },

    /// a non-terminating Call instruction (the call does not have an
    /// exceptional branch) This instruction is not in the Mu spec, but is
    /// documented in the HOL formal spec
    ExprCall {
        data: CallData,
        /// true to abort, false to rethrow
        is_abort: bool,
    },

    EpxrCCall {
        data: CallData,
        is_abort: bool,
    },

    Load {
        is_ptr: bool,
        order: MemoryOrder,
        arg: Value,
    },

    Store {
        is_ptr: bool,
        order: MemoryOrder,
        args: [Value; 2],
    },

    CmpXchg {
        is_ptr: bool,
        is_weak: bool,
        success: MemoryOrder,
        fail: MemoryOrder,
        args: [Value; 3],
    },

    New(P<Type>),
    Alloca(P<Type>),
    AllocaU(P<Type>),
    AllocaUHybrid(P<Type>, Value),

    NewHybrid(P<Type>, Value),

    AllocaHybrid(P<Type>, Value),

    NewStack(Value),
    KillStack(Value),

    CurrentStack,

    NewThread {
        stack: Value,
        thread_local: Option<Value>,
        is_exception: bool,
        args: ValueList,
    },

    NewFrameCursor(Value),

    GetIRef(Value),
    GetFieldIRef {
        is_ptr: bool,
        base: Value,
        index: usize,
    },

    GetElementIRef {
        is_ptr: bool,
        base: Value,
        index: Value,
    },

    ShiftIRef {
        is_ptr: bool,
        base: Value,
        offset: Value,
    },

    GetVarPartIRef {
        is_ptr: bool,
        base: Value,
    },

    Fence(MemoryOrder),
    Return(ValueList),
    ThreadExit,

    Throw(Value),
    TailCall(CallData),

    Jump {
        block: BlockCall,
    },

    Branch {
        value: Value,
        blocks: [BlockCall; 2],
        likeness: u32,
    },

    Select {
        value: Value,
        if_true: Value,
        if_false: Value,
    },

    Watchpoint {
        id: Option<u64>,
        disabled: Option<BlockCall>,
        resume: ResumptionData,
        keepalive: Option<ValueList>,
    },

    WatchpointBranch {
        wp: u64,
        blocks: [BlockCall; 2],
    },

    Call {
        data: CallData,
        resume: ResumptionData,
    },

    CCall {
        data: CallData,
        resume: ResumptionData,
    },

    SwapStackExc {
        stack: Value,
        is_exception: bool,
        args: ValueList,
        resume: ResumptionData,
    },

    SwapStackExpr {
        stack: Value,
        is_exception: bool,
        args: ValueList,
    },

    Switch {
        cond: Value,
        default: Value,
        jump_table: JumpTable,
    },

    Move(Value),

    /// Call intrinsic function.
    ///
    /// Equal to `common_inst`.
    CallIntrinsic {
        name: u32,
        args: ValueList,
        resume: Option<ResumptionData>,
    },
}
