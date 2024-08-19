//! Acrhitecture-specific types and functions
//!
//!
//! Some of the code is generated using macroassembler so we get more or less portable code.

#[cfg(target_arch = "x86_64")]
pub mod x86_64;

#[cfg(target_arch = "x86_64")]
pub use x86_64::prelude::*;

#[cfg(target_arch = "aarch64")]
pub mod aarch64;
#[cfg(target_arch = "aarch64")]
pub use aarch64::prelude::*;
