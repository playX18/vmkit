//! Practical implementation of On-Stack Replacement
//!
//! Based on [Hop, Skip, & Jump](https://dl.acm.org/doi/10.1145/3296975.3186412) paper.

use mmtk::util::Address;

use crate::{
    arch::{
        x86_64::{ROPFrame, StackTop},
        CalleeSaves,
    },
    threads::stack::Stack,
};

use super::thunks::BEGIN_RESUME;

pub trait Unwinder {
    type Error;

    /// Advance the unwinder by one frame.
    ///
    /// Returns whether there's more frames or no, or an error.
    fn step(&mut self) -> Result<bool, Self::Error>;

    fn callee_saves(&mut self) -> CalleeSaves;

    fn ip(&mut self) -> Address;
    fn sp(&mut self) -> Address;
    fn set_ip(&mut self, ip: Address);
    fn set_sp(&mut self, sp: Address);
}

pub struct FrameCursor<'a, U> {
    unwinder: U,
    stack: &'a mut Stack,
}

impl<'a, U: Unwinder> FrameCursor<'a, U> {
    pub fn new(unwinder: U, stack: &'a mut Stack) -> Self {
        Self { unwinder, stack }
    }

    pub fn unwinder(&self) -> &U {
        &self.unwinder
    }
    pub fn unwinder_mut(&mut self) -> &mut U {
        &mut self.unwinder
    }

    /// Move the frame cursor to the next frame, moving down the stack from called to caller.
    pub fn next_frame(&mut self) -> Result<bool, U::Error> {
        self.unwinder.step()
    }

    /// Remove all frames above the current frame of the given frame cursor.
    ///
    /// # Safety
    ///
    /// Inheretely unsafe since can corrupt call-stack.
    pub unsafe fn pop_frames_to(&mut self) {
        let sp = self.unwinder.sp();
        self.stack.set_sp(sp);
        self.reconstruct_stackswap_top();
    }

    /// Create a new frame on the top of the stack pointed by the frame cursor.
    ///
    /// # Safety
    ///
    /// Inheretely unsafe since can corrupt call-stack.
    pub unsafe fn push_frame(&mut self, func: Address, adapter: Address) {
        let ip = self.unwinder.ip();
        let sp = self.unwinder.sp();

        self.stack.set_sp(sp);
        let rop_frame = self.stack.push::<ROPFrame>().as_mut().unwrap();

        rop_frame.func = func;
        rop_frame.saved_ret = ip;

        self.unwinder.set_ip(
            adapter
                .is_zero()
                .then_some(Address::from_ptr(BEGIN_RESUME.start()))
                .unwrap_or(adapter),
        );
        self.unwinder.set_sp(self.stack.sp());

        self.reconstruct_stackswap_top();
    }

    /// Reconstruct stackswap top to point to current frame.
    ///
    /// # Safety
    ///
    /// Unsafe because can corrupt call-stack.
    pub unsafe fn reconstruct_stackswap_top(&mut self) {
        let ss_top = self.stack.push::<StackTop>().as_mut().unwrap();

        //ss_top.ss_cont = swapstack_cont as usize;

        ss_top.callee_saves = self.unwinder.callee_saves();
        ss_top.ret = self.unwinder.ip();
    }
}
