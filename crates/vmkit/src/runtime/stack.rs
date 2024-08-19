//! # Stack representation
//!
//! This module defines stacks and allow to create them, destroy or swap them.

use mmtk::util::Address;
use std::{alloc::Layout, mem::ManuallyDrop};

/// A stack status. Indicates whether stack is active, terminated, new or suspended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum StackStatus {
    /// Stack is new. The entrypoint function is not yet
    /// executed, any modifications or pushing of new frames is not allowed.  
    /// Only possible to initialize entrypoint.
    New = 0,
    /// Stack is active. Means it is currently executing and it is unsafe to trace or modify
    /// from other threads. It is safe to trace from the same thread.
    Active = 1,
    /// Stack is suspended. It is safe to trace and modify from any threads that can get a reference to it.
    Suspended = 2,
    /// Stack is terminated. It essentially means the exit-point is reached and you can't do
    /// anything meaningful with the stack.
    Terminated = 3,
}

#[repr(C)]
pub struct Stack {
    /// Stack-pointer of this stack. Points to last SP known.
    sp: Address,
    size: usize,
    start: Address,
    pub(crate) state: StackStatus,
}

impl Stack {
    pub const EMPTY: Self =
        unsafe { Self::from_raw(Address::zero(), Address::zero(), 0, StackStatus::New) };

    /// Construct stack from raw elements.
    ///
    /// # Safety
    ///
    /// At least `sp` must be a valid value. `state`, `start`, `size` are not important to enter the stack
    /// but `state` is important for unwinding.
    pub const unsafe fn from_raw(
        start: Address,
        sp: Address,
        size: usize,
        state: StackStatus,
    ) -> Self {
        Self {
            sp,
            size,
            start,
            state,
        }
    }

    pub const fn sp(&self) -> Address {
        self.sp
    }

    pub const fn start(&self) -> Address {
        self.start
    }

    /// Current stack size.
    ///
    /// NOTE: Allowed to be zero.
    pub const fn size(&self) -> usize {
        self.size
    }

    pub unsafe fn set_sp(&mut self, sp: Address) {
        self.sp = sp;
    }

    /// Update the stack size.
    ///
    /// # Safety
    ///
    /// Potentially breaks code which relies on `start` and `size`.
    pub unsafe fn set_size(&mut self, size: usize) {
        self.size = size;
    }

    pub const fn state(&self) -> StackStatus {
        self.state
    }

    /// Update stack state.
    ///
    /// # Safety
    ///
    /// Unsafe because unwinding could break with incorrect state.
    pub unsafe fn set_state(&mut self, state: StackStatus) {
        self.state = state;
    }

    pub fn push<T>(&mut self) -> *mut T {
        self.sp -= size_of::<T>();

        self.sp.to_mut_ptr()
    }

    pub fn new(size: usize) -> Self {
        unsafe {
            let mem = std::alloc::alloc_zeroed(Layout::from_size_align_unchecked(
                size,
                size_of::<usize>() * 2,
            ));

            let start = Address::from_mut_ptr(mem);

            Self {
                size,
                start,
                sp: start + size,
                state: StackStatus::New,
            }
        }
    }

    pub unsafe fn dealloc(self) {
        if self.size == 0 || self.start.is_zero() {
            panic!("invalid stack to dealloc");
        }
        std::alloc::dealloc(
            self.start.to_mut_ptr(),
            Layout::from_size_align_unchecked(self.size, size_of::<usize>() * 2),
        );
    }
}

#[repr(C)]
pub enum ValueLocation {
    FPR(f64),
    GPR(usize),
}

/// A managed by VMKit stack. This stack is properly allocated using memory mapping
/// and can be used to spawn a thread in it.
#[repr(C)]
pub struct ManagedStack {
    stack: Stack,
    mmap: ManuallyDrop<memmap2::MmapMut>,
}

const STACK_SIZE: usize = 4 * 1024 * 1024;

impl ManagedStack {
    pub fn new() -> Result<Self, std::io::Error> {
        let mmap = memmap2::MmapMut::map_anon(STACK_SIZE)?;
        mmap.advise(memmap2::Advice::Sequential)?;
        let start = Address::from_ptr(mmap.as_ptr());
        let sp = start + STACK_SIZE;
        Ok(Self {
            mmap: ManuallyDrop::new(mmap),
            stack: unsafe { Stack::from_raw(start, sp, STACK_SIZE, StackStatus::New) },
        })
    }

    pub fn mmap(&self) -> &memmap2::MmapMut {
        &self.mmap
    }

    pub fn stack(&self) -> &Stack {
        &self.stack
    }

    pub unsafe fn stack_mut(&mut self) -> &mut Stack {
        &mut self.stack
    }
}
