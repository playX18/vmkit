use std::{mem::offset_of, num::NonZeroUsize, ptr::null_mut};

use mmtk::util::{constants::BYTES_IN_PAGE, conversions::raw_align_up, Address};

use crate::{
    arch::{
        x86_64::{InitialStackTop, StackTop},
        CalleeSaves,
    },
    runtime::thunks::{BEGIN_RESUME, SWAPSTACK_CONT},
};

/// Stack represents metadata for a VMKit stack.
/// A VMKit stack is explicitly different from a native stack that comes with the
/// thread from OS, and is managed by the VM. A VMKit stack is logically
/// independent from a VMKit thread, as we allow creation of stacks to be
/// independent of thread creation, we allow binding stack to a new thread and
/// swap stack to rebind stacks. A VMKit stack is seen as a piece of memory
/// that contains function execution records.
///
///
/// VMKit stack has a layout as below:
/// ```text
///                              <- stack grows this way <-
///    lo addr                                                    hi addr
///     | overflow guard page | actual stack ..................... | underflow
/// guard page|     |                     |                                    |
/// |
/// ``````
/// We use guard page for overflow/underflow detection.
///
/// Stacks also can be used to represent coroutine context, whenever you swap one stack
/// with another the `link` field is changed to old stack so if you want to imlement `coro::yield()`
/// you can simply `swapstack(cur_stack.link())`.
/// Stack lifetime management is offloaded to the users of VMKit.
///
///
/// Original code for the Stack type comes from ZebuVM, it's adapted to be more usable
/// in VMKit context.
pub struct Stack {
    size: usize,
    overflow_guard: Address,
    lower_bound: Address,
    upper_bound: Address,
    underflow_guard: Address,

    pub(super) sp: Address,
    bp: Address,
    ip: Address,

    state: StackState,
    link: *mut Stack,
    #[allow(dead_code)]
    mmap: Option<memmap2::MmapMut>,
}

/// 4 MB
pub const DEFAULT_STACK_SIZE: usize = 4 << 20;

impl Stack {
    pub const SP_OFFSET: usize = offset_of!(Self, sp);
    pub const IP_OFFSET: usize = offset_of!(Self, ip);
    pub const BP_OFFSET: usize = offset_of!(Self, bp);
    pub const LINK_OFFSET: usize = offset_of!(Self, link);
    pub const STATE_OFFSET: usize = offset_of!(Self, state);

    pub fn new(stack_size: Option<NonZeroUsize>) -> Self {
        // allocate memory for the stack
        let stack_size = raw_align_up(
            stack_size
                .map(NonZeroUsize::get)
                .unwrap_or(DEFAULT_STACK_SIZE),
            BYTES_IN_PAGE,
        );
        let mut anon_mmap = {
            // reserve two guard pages more than we need for the stack
            let total_size = BYTES_IN_PAGE * 2 + stack_size;
            match memmap2::MmapMut::map_anon(total_size) {
                Ok(m) => m,
                Err(_) => panic!("failed to mmap for a stack"),
            }
        };

        let mmap_start = Address::from_ptr(anon_mmap.as_mut_ptr());
        debug_assert!(mmap_start.is_aligned_to(BYTES_IN_PAGE));

        // calculate the addresses
        let overflow_guard = mmap_start;
        let lower_bound = mmap_start + BYTES_IN_PAGE;
        let upper_bound = lower_bound + stack_size;
        let underflow_guard = upper_bound;

        // protect the guard pages
        mmtk::util::memory::mprotect(overflow_guard, BYTES_IN_PAGE)
            .expect("failed to protect overflow guard");
        mmtk::util::memory::mprotect(underflow_guard, BYTES_IN_PAGE)
            .expect("failed to protect underflow guard");

        let sp = upper_bound;

        let this = Stack {
            state: StackState::New,

            size: stack_size,
            overflow_guard,
            lower_bound,
            upper_bound,
            underflow_guard,

            sp,
            link: null_mut(),
            bp: upper_bound,
            ip: unsafe { Address::zero() },

            mmap: Some(anon_mmap),
        };
        //unsafe { this.initialize(entrypoint, Address::from_mut_ptr(BEGIN_RESUME.start())) }
        this
    }

    pub unsafe fn uninit() -> Self {
        Self {
            bp: Address::ZERO,
            ip: Address::ZERO,
            lower_bound: Address::ZERO,
            mmap: None,
            overflow_guard: Address::ZERO,
            size: 0,
            sp: Address::ZERO,
            state: StackState::Unknown,
            underflow_guard: Address::zero(),
            upper_bound: Address::ZERO,
            link: null_mut(),
        }
    }

    /// Link to a stack that switched to this one. Can be used to quickly
    /// switch back to previous stack.
    pub fn link(&self) -> *mut Stack {
        self.link
    }

    /// Set a link to previous stack.
    ///
    /// # Safety
    ///
    /// Unsafe, `link` pointer is not verified by us at all, it's up to the user
    /// to provide proper stack link pointer.
    pub unsafe fn set_link(&mut self, link: *mut Stack) {
        self.link = link;
    }

    /// Initialize stack with `entrypoint` to be executed once we swap to it
    /// and `adapter` to be executed before `entrypoint` to setup various registers.
    ///
    /// # Safety
    ///
    /// entrypoint and adapters are not verified and might as well break the stack,
    /// ensure that they are correct and do not violate current platforms ABI.
    pub unsafe fn initialize(&mut self, entrypoint: Address, adapter: Address) {
        self.sp -= size_of::<InitialStackTop>();
        unsafe {
            let adapter = adapter
                .is_zero()
                .then_some(Address::from_ptr(BEGIN_RESUME.start()))
                .unwrap_or(adapter);
            let stack_top = self.sp.as_mut_ref::<InitialStackTop>();

            stack_top.ss_top.ss_cont = SWAPSTACK_CONT.start() as _;
            stack_top.ss_top.ret = adapter;
            stack_top.rop.func = entrypoint;
        }
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn lower_bound(&self) -> Address {
        self.lower_bound
    }

    pub fn upper_bound(&self) -> Address {
        self.upper_bound
    }

    pub fn overflow_guard(&self) -> Address {
        self.overflow_guard
    }

    pub fn underflow_guard(&self) -> Address {
        self.underflow_guard
    }

    pub fn state(&self) -> StackState {
        self.state
    }

    pub fn ip(&self) -> Address {
        self.ip
    }

    pub fn sp(&self) -> Address {
        self.sp
    }

    pub fn bp(&self) -> Address {
        self.bp
    }

    pub unsafe fn stack_top_ip(&self) -> Address {
        self.sp.as_ref::<StackTop>().ret
    }

    pub unsafe fn set_stack_top_ip(&mut self, ip: Address) {
        self.sp.as_mut_ref::<StackTop>().ret = ip;
    }

    pub unsafe fn callee_saves(&self) -> &CalleeSaves {
        &self.sp.as_ref::<StackTop>().callee_saves
    }

    pub unsafe fn callee_saves_mut(&mut self) -> &mut CalleeSaves {
        &mut self.sp.as_mut_ref::<StackTop>().callee_saves
    }

    pub unsafe fn push<T>(&mut self) -> *mut T {
        self.sp -= size_of::<T>();
        self.sp.to_mut_ptr()
    }

    pub unsafe fn set_sp(&mut self, sp: Address) {
        self.sp = sp;
    }

    /// Reset stack by setting it's sp to the stack start.
    ///
    /// # Safety
    ///
    /// Does not zero the stack, execute destructors etc. Up to the user to do all of it.
    pub unsafe fn reset(&mut self) {
        self.sp = self.upper_bound();
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum StackState {
    New,
    Ready,
    Active,
    Dead,
    Unknown,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ValueLocation {
    GPR(usize),
    FPR(f64),
    GPREx(usize, usize),
}

/// A type on the stack.
///
/// This is used to properly compute parameters and return values when swapping stack.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum StackType {
    Float,
    #[default]
    Int,
    Int128,
    Float128,
}

#[cfg(test)]
mod tests {

    use mmtk::util::Address;

    use crate::{
        mock::{MockThread, MockVM},
        runtime::{
            threads::{stack::StackState, terminate_thread, vmkit_current_stack, Thread},
            thunks::{swapstack, swapstack_kill},
        },
    };

    use super::Stack;

    unsafe extern "C" fn entrypoint(arg: usize) -> ! {
        let x = arg * 2;
        assert_eq!(arg, 42);

        let current = vmkit_current_stack::<MockVM>();
        let y = swapstack::<MockVM>((*current).link(), x);

        let z = y + 1;
        assert_eq!(y, 85);
        swapstack_kill::<MockVM>((*current).link(), z);

        std::hint::unreachable_unchecked();
    }

    extern "C" fn test_stack_main(arg: u64) {
        assert_eq!(arg, 42);

        let coro = Box::into_raw(Box::new(Stack::new(None)));
        unsafe {
            (*coro).initialize(Address::from_ptr(entrypoint as *const u8), Address::ZERO);
        }

        let res = unsafe { swapstack::<MockVM>(coro, arg as _) };
        assert_eq!(res, 84);
        let res2 = unsafe { swapstack::<MockVM>(coro, res + 1) };
        assert_eq!(res2, 86);

        unsafe {
            println!("coro state: {:?}", (*coro).state());
            assert_eq!((*coro).state(), StackState::Dead);
            let _ = Box::from_raw(coro);
        }

        unsafe {
            terminate_thread::<MockVM>();
        }
    }

    #[test]
    fn test_stack() {
        println!("in main: {}", 42);
        let (handle, main_thread) = MockThread::spawn(test_stack_main, 42);
        let _ = handle.unwrap().join();
        MockThread::kill(main_thread);
    }
}
