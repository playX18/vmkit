//! Unwinding on top of framehop
//!
//!
//! Simple interface for unwinding on top of framehop. Implements methods to register custom modules
//! and currently linked modules (to current process).
//!

use crate::arch::CalleeSaves;

use crate::threads::stack::*;
use framehop::{AllocationPolicy, Module, Unwinder as _, UnwinderNative};

use mmtk::util::Address;
pub mod object;

pub use framehop::{self, CacheNative, FrameAddress};

pub struct Unwinder<'a, P>
where
    P: AllocationPolicy,
{
    unwinder: UnwinderNative<&'a [u8], P>,
}

impl<'a, P: AllocationPolicy> Unwinder<'a, P> {
    pub fn new() -> Self {
        Self {
            unwinder: UnwinderNative::new(),
        }
    }

    pub fn add_current_module(&mut self) {
        for obj in object::get_objects() {
            self.unwinder.add_module(obj.to_module());
        }
    }

    pub fn add_module(&mut self, module: Module<&'a [u8]>) {
        self.unwinder.add_module(module);
    }

    #[cfg(target_arch = "x86_64")]
    pub fn iter_frames_of<'u, 'c>(
        &'u self,
        stack: &Stack,
        cache: &'c mut CacheNative<P>,
    ) -> UnwindIterator<'u, 'c, UnwinderNative<&'a [u8], P>> {
        let ip = stack.ip();

        UnwindIterator::new(
            &self.unwinder,
            ip.as_usize() as _,
            unsafe { stack.unwind_regs() },
            cache,
        )
    }

    #[cfg(target_arch = "x86_64")]
    pub fn iter_frames<'u, 'c>(
        &'u mut self,
        cache: &'c mut CacheNative<P>,
    ) -> UnwindIterator<'u, 'c, UnwinderNative<&'a [u8], P>> {
        use framehop::UnwindRegsNative;

        #[allow(unused)]
        let (rip, regs) = {
            let mut rip = 0;
            let mut rsp = 0;
            let mut rbp = 0;
            unsafe {
                std::arch::asm!("lea {}, [rip]", out(reg) rip);
                std::arch::asm!("mov {}, rsp", out(reg) rsp);
                std::arch::asm!("mov {}, rbp", out(reg) rbp);
            }
            (rip, UnwindRegsNative::new(rip, rsp, rbp))
        };
        UnwindIterator::new(&self.unwinder, rip, regs, cache)
    }
}

enum UnwindIteratorState {
    Initial(u64),
    Unwinding(FrameAddress),
    Done,
}

pub struct UnwindIterator<'u, 'c, U: framehop::Unwinder + ?Sized> {
    unwinder: &'u U,
    state: UnwindIteratorState,
    regs: U::UnwindRegs,
    cache: &'c mut U::Cache,
}

impl<'u, 'c, U: framehop::Unwinder + ?Sized> UnwindIterator<'u, 'c, U> {
    /// Create a new iterator. You'd usually use [`Unwinder::iter_frames`] instead.
    pub fn new(unwinder: &'u U, pc: u64, regs: U::UnwindRegs, cache: &'c mut U::Cache) -> Self {
        Self {
            unwinder,
            state: UnwindIteratorState::Initial(pc),
            regs,
            cache,
        }
    }

    pub fn regs(&self) -> &U::UnwindRegs {
        &self.regs
    }

    pub fn regs_mut(&mut self) -> &mut U::UnwindRegs {
        &mut self.regs
    }

    /// Yield the next frame in the stack.
    ///
    /// The first frame is `Ok(Some(FrameAddress::InstructionPointer(...)))`.
    /// Subsequent frames are `Ok(Some(FrameAddress::ReturnAddress(...)))`.
    ///
    /// If a root function has been reached, this iterator completes with `Ok(None)`.
    /// Otherwise it completes with `Err(...)`, usually indicating that a certain stack
    /// address could not be read.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<FrameAddress>, framehop::Error> {
        let next = match self.state {
            UnwindIteratorState::Initial(pc) => {
                self.state = UnwindIteratorState::Unwinding(FrameAddress::InstructionPointer(pc));
                return Ok(Some(FrameAddress::InstructionPointer(pc)));
            }
            UnwindIteratorState::Unwinding(address) => self.unwinder.unwind_frame(
                address,
                &mut self.regs,
                self.cache,
                &mut |addr| unsafe { Ok((addr as *const u64).read()) },
            )?,
            UnwindIteratorState::Done => return Ok(None),
        };
        match next {
            Some(return_address) => {
                let return_address = FrameAddress::from_return_address(return_address)
                    .ok_or(framehop::Error::ReturnAddressIsNull)?;
                self.state = UnwindIteratorState::Unwinding(return_address);
                Ok(Some(return_address))
            }
            None => {
                self.state = UnwindIteratorState::Done;
                Ok(None)
            }
        }
    }
}

impl<'u, 'c, P: AllocationPolicy> super::osr::Unwinder
    for UnwindIterator<'u, 'c, UnwinderNative<&'u [u8], P>>
{
    type Error = framehop::Error;
    fn callee_saves(&mut self) -> crate::arch::CalleeSaves {
        #[cfg(target_arch = "x86_64")]
        {
            use framehop::x86_64::Reg::*;
            #[cfg(not(windows))]
            {
                CalleeSaves {
                    r15: self.regs.get(R15),
                    r14: self.regs.get(R14),
                    r13: self.regs.get(R13),
                    r12: self.regs.get(R12),
                    rbx: self.regs.get(RBX),
                    rbp: self.regs.get(RBP),
                }
            }

            #[cfg(windows)]
            {
                CalleeSaves {
                    r15: self.regs.get(R15),
                    r14: self.regs.get(R14),
                    r13: self.regs.get(R13),
                    r12: self.regs.get(R12),
                    rsi: self.regs.get(RSI),
                    rdi: self.regs.get(RDI),
                    rbx: self.regs.get(RBX),
                    rbp: self.regs.get(RBP),
                }
            }
        }
    }

    fn step(&mut self) -> Result<bool, Self::Error> {
        self.next().map(|x| x.is_some())
    }

    fn ip(&mut self) -> mmtk::util::Address {
        Address::from_ptr(self.regs().ip() as *const u8)
    }

    fn set_ip(&mut self, ip: mmtk::util::Address) {
        self.regs.set_ip(ip.as_usize() as _);
    }

    fn set_sp(&mut self, sp: Address) {
        self.regs.set_sp(sp.as_usize() as _);
    }

    fn sp(&mut self) -> Address {
        Address::from_ptr(self.regs().sp() as *const u8)
    }
}
