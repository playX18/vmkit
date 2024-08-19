use std::ops::Add;

use framehop::x86_64::{Reg, UnwindRegsX86_64};
use mmtk::util::Address;

use crate::runtime::stack::{Stack, StackStatus, ValueLocation};

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

#[cfg(not(windows))]
#[naked_function::naked]
pub unsafe extern "C" fn swapstack(from: &mut Stack, to: &mut Stack, argument: usize) -> usize {
    asm! {
        "
        push rbp
        mov rbp, rsp

        push rbx
        push r12
        push r13
        push r14
        push r15

        lea rcx, [rip + {cont}]
        push rcx

        mov [rdi], rsp
        # set 'from' to suspended and `to` to active
        mov byte ptr [rdi+24], 2
        mov byte ptr [rsi+24], 1
        mov rsp, [rsi]

        # move argument to return reg
        mov rax, rdx
        ret
        ",
        cont = sym swapstack_cont,
    }
}

#[cfg(windows)]
#[naked_function::naked]
pub unsafe extern "C" fn swapstack(from: &mut Stack, to: &mut Stack, argument: usize) -> usize {
    asm! {
        "
        push rbp
        mov rbp, rsp

        push rbx
        push rsi
        push rdi
        push r12
        push r13
        push r14
        push r15

        lea rax, [rip + {cont}]
        push rax

        mov [rcx], rsp
        # set 'from' to suspended and `to` to active
        mov byte ptr [rcx+24], 2
        mov byte ptr [rdx+24], 1
        mov rsp, [rdx]

        # move argument to return reg
        mov rax, r8
        ret
        ",
        cont = sym swapstack_cont,
    }
}

#[cfg(not(windows))]
#[naked_function::naked]
pub unsafe extern "C" fn swapstack_cont() -> usize {
    asm! {
        "
        pop r15
        pop r14
        pop r13
        pop r12
        pop rbx

        pop rbp
        ret
        "
    }
}

#[cfg(windows)]
#[naked_function::naked]
pub unsafe extern "C" fn swapstack_cont() -> usize {
    asm! {
        "
        pop r15
        pop r14
        pop r13
        pop r12
        pop rdi
        pop rsi
        pop rbx

        pop rbp
        ret
        "
    }
}
#[cfg(not(windows))]
#[naked_function::naked]
pub unsafe extern "C" fn begin_resume(value: usize) -> usize {
    asm! {
        "
        mov rdi, rax
        ret
        "
    }
}

#[cfg(windows)]
#[naked_function::naked]
pub unsafe extern "C" fn begin_resume(value: usize) -> usize {
    asm! {
        "
        mov rdi, rax
        ret
        "
    }
}

#[cfg(not(windows))]
#[naked_function::naked]
pub unsafe extern "C" fn thread_start() {
    asm! {
        "
        push rbp
        mov rbp, rsp

        push rbx
        push r12
        push r13
        push r14
        push r15

        lea rax, [rip + {cont}]
        push rax

        mov [rcx], rsp
        # set 'from' to suspended and `to` to active
        mov byte ptr [rcx+24], 2
        mov byte ptr [rdx+24], 1
        mov rsp, [rdx]

        pop r9
        pop r8
        pop rcx
        pop rdx
        pop rsi
        pop rdi
        movsd 0(%rsp), %xmm7
        movsd 8(%rsp), %xmm6
        movsd 16(%rsp), %xmm5
        movsd 24(%rsp), %xmm4
        movsd 32(%rsp), %xmm3
        movsd 40(%rsp), %xmm2
        movsd 48(%rsp), %xmm1
        movsd 56(%rsp), %xmm0
        add $64, %rsp

        mov rbp, rsp
        ret
        ",
        cont = sym swapstack_cont
    }
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
    pub use super::{begin_resume, swapstack, swapstack_cont};
}

impl Stack {
    pub unsafe fn init(&mut self, func: Address, adapter: Address) {
        let stack_top = &mut *self.push::<InitialStackTop>();

        stack_top.ss_top.ss_cont = swapstack_cont as _;
        stack_top.ss_top.ret = adapter;
        stack_top.rop.func = func;
    }

    pub fn init_simple(&mut self, func: extern "C" fn(usize) -> usize) {
        unsafe {
            self.init(
                Address::from_ptr(func as *const u8),
                Address::from_ptr(begin_resume as *const u8),
            )
        }
    }

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
