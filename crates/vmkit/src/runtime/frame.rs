use std::marker::PhantomData;

use mmtk::util::Address;

use crate::Runtime;

pub trait StackFrame<R: Runtime> {
    fn fp(&self) -> Address;
    fn pc(&self) -> Address;
}

/// A C Stack frame implementation. Can be used when your VM has a standard C ABI everywhere.
pub struct CFrame<R: Runtime>(PhantomData<R>);
