use crate::{
    arch::*,
    raw::{swapstack_begin_resume, swapstack_cont},
    stack_bounds::StackBounds,
};
use easy_bitfield::{BitField, BitFieldTrait};
use std::{mem::MaybeUninit, num::NonZeroUsize, ptr::null_mut};

use crate::utils::raw_align_up;

type StackIsNative = BitField<u8, bool, 0, 1, false>;
type StackIsMapped = BitField<u8, bool, 1, 1, false>;

#[repr(C)]
pub struct Stack {
    sp: *mut u8,
    overflow_guard: *mut u8,
    lower_bound: *mut u8,
    upper_bound: *mut u8,
    underflow_guard: *mut u8,
    size: usize,
    bp: *mut u8,
    ip: *mut u8,
    state: StackState,
    user_data: *mut (),
    flags: u8,
    #[allow(dead_code)]
    mmap: Option<memmap2::MmapMut>,
}

impl Drop for Stack {
    fn drop(&mut self) {
        if self.is_mapped() {
            if let Some(map) = self.mmap.take() {
                drop(map);
            }
        }
    }
}

/// 4 MB
pub const DEFAULT_STACK_SIZE: usize = 4 << 20;

impl Stack {
    pub fn is_native(&self) -> bool {
        StackIsNative::decode(self.flags)
    }

    pub fn is_mapped(&self) -> bool {
        StackIsMapped::decode(self.flags)
    }

    pub unsafe fn set_native(&mut self, value: bool) {
        self.flags = StackIsNative::update(value, self.flags);
    }

    pub unsafe fn set_mapped(&mut self, value: bool) {
        self.flags = StackIsMapped::update(value, self.flags);
    }

    pub fn new(stack_size: Option<NonZeroUsize>) -> Self {
        // allocate memory for the stack
        let stack_size = raw_align_up(
            stack_size
                .map(NonZeroUsize::get)
                .unwrap_or(DEFAULT_STACK_SIZE),
            page_size::get(),
        );
        let mut anon_mmap = {
            // reserve two guard pages more than we need for the stack
            let total_size = page_size::get() * 2 + stack_size;
            match memmap2::MmapMut::map_anon(total_size) {
                Ok(m) => m,
                Err(_) => panic!("failed to mmap for a stack"),
            }
        };

        let mmap_start = anon_mmap.as_mut_ptr();

        unsafe {
            // calculate the addresses
            let overflow_guard = mmap_start;
            let lower_bound = mmap_start.add(page_size::get());
            let upper_bound = lower_bound.add(stack_size);
            let underflow_guard = upper_bound;

            // protect the guard pages

            #[cfg(unix)]
            {
                libc::mprotect(overflow_guard as _, page_size::get(), libc::PROT_NONE);
                libc::mprotect(underflow_guard as _, page_size::get(), libc::PROT_NONE);
            }
            let sp = upper_bound;

            let this = Stack {
                state: StackState::New,
                size: stack_size,
                overflow_guard,
                lower_bound,
                upper_bound,
                underflow_guard,

                sp,
                flags: StackIsNative::update(false, StackIsMapped::encode(true)),
                bp: upper_bound,
                ip: null_mut(),
                user_data: null_mut(),

                mmap: Some(anon_mmap),
            };

            this
        }
    }

    pub fn set_user_data(&mut self, data: *mut ()) {
        self.user_data = data;
    }

    pub fn take_user_data(&mut self) -> *mut () {
        std::mem::replace(&mut self.user_data, null_mut())
    }

    pub fn user_data(&self) -> *mut () {
        self.user_data
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn lower_bound(&self) -> *mut u8 {
        self.lower_bound
    }

    pub fn upper_bound(&self) -> *mut u8 {
        self.upper_bound
    }

    pub fn overflow_guard(&self) -> *mut u8 {
        self.overflow_guard
    }

    pub fn underflow_guard(&self) -> *mut u8 {
        self.underflow_guard
    }

    pub fn state(&self) -> StackState {
        self.state
    }

    pub fn sp(&self) -> *mut u8 {
        self.sp
    }

    pub unsafe fn set_sp(&mut self, sp: *mut u8) {
        self.sp = sp;
    }

    pub unsafe fn stack_top_ip(&self) -> *const u8 {
        (*self.sp.cast::<StackTop>()).ret_addr() as _
    }

    pub unsafe fn set_stack_top_ip(&mut self, ip: *const u8) {
        (*self.sp.cast::<StackTop>()).set_ret_addr(ip as _);
    }

    pub unsafe fn callee_saves(&self) -> &CalleeSaves {
        //&self.sp.as_ref::<StackTop>().callee_saves
        &(&*self.sp.cast::<StackTop>()).callee_saves
    }

    pub unsafe fn callee_saves_mut(&mut self) -> &mut CalleeSaves {
        &mut (&mut *self.sp.cast::<StackTop>()).callee_saves
    }

    pub unsafe fn push<T>(&mut self) -> *mut T {
        self.sp = self.sp.sub(size_of::<T>());
        self.sp.cast()
    }

    pub unsafe fn pop<T>(&mut self) {
        self.sp = self.sp.add(size_of::<T>());
    }

    /// Initialize stack with `entrypoint` to be executed once we swap to it
    /// and `adapter` to be executed before `entrypoint` to setup various registers.
    ///
    /// # Safety
    ///
    /// entrypoint and adapters are not verified and might as well break the stack,
    /// ensure that they are correct and do not violate current platforms ABI.
    pub unsafe fn initialize(
        &mut self,
        entrypoint: extern "C-unwind" fn(Transfer) -> Transfer,
        adapter: *const u8,
    ) {
        unsafe {
            let stack_top = self.push::<InitialStackTop>().as_mut().unwrap();
            let adapter = adapter
                .is_null()
                .then_some(swapstack_begin_resume as *const u8)
                .unwrap_or(adapter);

            stack_top.ss_top.ss_cont = swapstack_cont as _;
            stack_top.ss_top.set_ret_addr(adapter as _);
            stack_top.rop.func = entrypoint as _;
        }
    }

    pub fn from_native() -> Self {
        let current = StackBounds::current();
        Self {
            flags: StackIsNative::encode(true),
            lower_bound: current.bound(),
            upper_bound: current.origin(),
            size: current.origin() as usize - current.bound() as usize,
            mmap: None,
            overflow_guard: null_mut(),
            underflow_guard: null_mut(),
            sp: current_stack_pointer(),
            bp: null_mut(),
            ip: null_mut(),
            state: StackState::Active,
            user_data: null_mut(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
#[repr(C)]
pub enum StackState {
    New,
    Ready,
    Active,
    Dead,
    Unknown,
}

/// A structure representing transfer of control from one stack to another.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Transfer {
    /// Previous stack of execution.
    pub stack: *mut Stack,
    /// Data that was passed to `swapstack()` call.
    pub data: *mut (),
}

#[cold]
#[inline(never)]
pub extern "C" fn current_stack_pointer() -> *mut u8 {
    unsafe {
        let mut x = MaybeUninit::<*mut u8>::uninit();
        x.as_mut_ptr().write_volatile(&mut x as *mut _ as *mut u8);
        x.assume_init()
    }
}
