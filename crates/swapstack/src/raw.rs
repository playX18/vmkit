#![allow(improper_ctypes)]
use crate::stack::{Stack, Transfer};

extern "C" {
    pub fn swapstack(from: *mut Stack, to: *mut Stack, arg: *mut ()) -> Transfer;
    pub fn ontop_swapstack(
        from: *mut Stack,
        to: *mut Stack,
        arg: *mut (),
        f: extern "C-unwind" fn(Transfer) -> Transfer,
    ) -> Transfer;
    pub fn swapstack_begin_resume();
    pub fn swapstack_cont();
}
