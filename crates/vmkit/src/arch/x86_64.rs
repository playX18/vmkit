use framehop::x86_64::{Reg, UnwindRegsX86_64};
use mmtk::util::Address;

use crate::threads::stack::Stack;

/// Callee-save registers on current platform.
///
/// This struct is repr(C) and is laid-out from last callee-save being the first
/// field and the first calee-save being the last field, this is to allow for efficient
/// ASM routines to manipulate stacks.
#[cfg(windows)]
#[repr(C)]
pub struct CalleeSaves {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rbx: u64,
    pub rbp: u64,
}

/// Callee-save registers on current platform.
///
/// This struct is repr(C) and is laid-out from last callee-save being the first
/// field and the first calee-save being the last field, this is to allow for efficient
/// ASM routines to manipulate stacks.
#[cfg(not(windows))]
#[repr(C)]
pub struct CalleeSaves {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbx: u64,
    pub rbp: u64,
}

#[cfg(not(windows))]
#[repr(C)]
pub struct GPRArguments {
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub r8: u64,
    pub r9: u64,
}
#[cfg(not(windows))]
#[repr(C)]
pub struct FPRArguments {
    pub xmm0: f64,
    pub xmm1: f64,
    pub xmm2: f64,
    pub xmm3: f64,
    pub xmm4: f64,
    pub xmm5: f64,
    pub xmm6: f64,
    pub xmm7: f64,
}

#[repr(C)]
pub struct StackTop {
    pub ss_cont: usize,
    pub callee_saves: CalleeSaves,
    pub ret: Address,
}
/// Return oriented programming frame representation. This is used to implement `SWAPSTACK` operation.
#[repr(C)]
pub struct ROPFrame {
    /// A function we want to enter.
    pub func: Address,
    /// Saved return address we want to go to after `func` returns.
    pub saved_ret: Address,
}
#[repr(C)]
pub struct InitialStackTop {
    pub ss_top: StackTop,
    pub rop: ROPFrame,
}

#[repr(C)]
pub struct StackTopWithArguments {
    pub ss_cont: Address,
    pub callee_saves: CalleeSaves,
    pub gp_arguments: GPRArguments,
    pub fp_arguments: FPRArguments,
    pub ret: Address,
}

pub mod prelude {
    pub use super::CalleeSaves;
    //pub use super::{begin_resume, swapstack, swapstack_cont};
}
/*
impl Stack {
    pub unsafe fn init(&mut self, func: Address, adapter: Address) {
        let stack_top = &mut *self.push::<InitialStackTop>();

        /*stack_top.ss_top.ss_cont = swapstack_cont as _;
        stack_top.ss_top.ret = adapter;
        stack_top.rop.func = func;*/
    }

    pub fn init_simple(&mut self, func: extern "C" fn(usize) -> usize) {}

    pub fn ip(&self) -> Address {
        unsafe { self.sp().as_ref::<StackTop>().ret }
    }

    pub unsafe fn set_ip(&mut self, ip: Address) {
        unsafe {
            self.sp().as_mut_ref::<StackTop>().ret = ip;
        }
    }

    pub fn callee_saves(&self) -> &CalleeSaves {
        assert_eq!(
            self.state(),
            StackStatus::Suspended,
            "access to callee-saves is only allowed for suspended stacks"
        );
        unsafe { &self.sp().as_ref::<StackTop>().callee_saves }
    }

    pub unsafe fn callee_saves_mut(&mut self) -> &mut CalleeSaves {
        assert_eq!(
            self.state(),
            StackStatus::Suspended,
            "access to callee-saves is only allowed for suspended stacks"
        );
        &mut self.sp().as_mut_ref::<StackTop>().callee_saves
    }

    pub fn init_with_arguments(&mut self, func: usize, arguments: Vec<ValueLocation>) {}
}

impl Stack {
    pub fn unwind_regs(&self) -> UnwindRegsX86_64 {
        let ip = self.ip();
        let sp = self.sp();
        let callee_saves = self.callee_saves();
        let mut regs =
            UnwindRegsX86_64::new(ip.as_usize() as _, sp.as_usize() as _, callee_saves.rbp);

        #[cfg(not(windows))]
        {
            regs.set(Reg::R15, callee_saves.r15);
            regs.set(Reg::R14, callee_saves.r14);
            regs.set(Reg::R13, callee_saves.r13);
            regs.set(Reg::R12, callee_saves.r12);
            regs.set(Reg::RBX, callee_saves.rbx);
        }

        #[cfg(windows)]
        {
            regs.set(Reg::R15, callee_saves.r15);
            regs.set(Reg::R14, callee_saves.r14);
            regs.set(Reg::R13, callee_saves.r13);
            regs.set(Reg::R12, callee_saves.r12);
            regs.set(Reg::RDI, callee_saves.rdi);
            regs.set(Reg::RSI, callee_saves.rsi);
            regs.set(Reg::RBX, callee_saves.rbx);
        }

        regs
    }
}
*/

impl Stack {
    pub unsafe fn unwind_regs(&self) -> UnwindRegsX86_64 {
        let ip = self.stack_top_ip();
        let sp = self.sp();

        let callee_saves = self.callee_saves();
        let mut regs =
            UnwindRegsX86_64::new(ip.as_usize() as _, sp.as_usize() as _, callee_saves.rbp);

        #[cfg(not(windows))]
        {
            regs.set(Reg::R15, callee_saves.r15);
            regs.set(Reg::R14, callee_saves.r14);
            regs.set(Reg::R13, callee_saves.r13);
            regs.set(Reg::R12, callee_saves.r12);
            regs.set(Reg::RBX, callee_saves.rbx);
        }

        #[cfg(windows)]
        {
            regs.set(Reg::R15, callee_saves.r15);
            regs.set(Reg::R14, callee_saves.r14);
            regs.set(Reg::R13, callee_saves.r13);
            regs.set(Reg::R12, callee_saves.r12);
            regs.set(Reg::RDI, callee_saves.rdi);
            regs.set(Reg::RSI, callee_saves.rsi);
            regs.set(Reg::RBX, callee_saves.rbx);
        }

        regs
    }
}
