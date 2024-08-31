use cranelift_entity::PrimaryMap;
use mu_utils::rc::P;
use smallvec::SmallVec;

use crate::types::Signature;

use super::entities::{FunctionId, FunctionVersionId};

/// Function represents a Mu function (not a specific version of a function).
///
/// This type stores function signature and a list of function versions,
/// and its current version.
#[derive(Clone, Debug)]
pub struct Function {
    pub signature: P<Signature>,
    pub current_version: Option<FunctionVersionId>,
    pub versions: PrimaryMap<FunctionVersionId, Function>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FunctionAttr {
    Inline(InlineOption),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InlineOption {
    Always,
    Never,
    Auto,
}

/// A specific version of [`Function`].
/// It owns the entire IR for the function version.
pub struct FunctionVersion {
    pub id: FunctionVersionId,
    pub func_id: FunctionId,

    is_defined: bool,
    is_compiled: bool,
    pub attributes: SmallVec<[FunctionAttr; 1]>,
}
