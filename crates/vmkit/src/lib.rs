pub use mmtk;
pub mod arch;
pub mod compiler;
pub mod mm;
pub mod mock;
pub mod objectmodel;
pub mod options;
pub mod runtime;
pub mod sync;
pub mod utils;

pub type ThreadOf<R> = <R as Runtime>::Thread;
pub type SlotOf<R> = <R as Runtime>::Slot;
pub type VTableOf<R> = <R as Runtime>::VTable;

pub use runtime::{MMTKVMKit, Runtime, VMKit, VMKitBuilder};
