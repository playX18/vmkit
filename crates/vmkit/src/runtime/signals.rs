//! Signal handling
//!
//! Cross-platform library to handle signals. On Unix we rely on posix signal API, on windows we use exception API.

pub mod unix;
