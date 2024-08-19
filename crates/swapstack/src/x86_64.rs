use std::arch::global_asm;

global_asm! {
    "
    .global swapstack
    # swapstack(from: &mut Stack, to: &mut Stack, arg1: usize, arg2: usize) -> (usize, usize)
    swapstack:
        push rbp
        mov rsp, rbp

        push rbx
        push r12
        push r13
        push r14
        push r15

        lea rax, [rip + swapstack_cont_local]
        push rax 
        mov [rdi], rsp
        mov rsp, [rdx]
        mov rax, rdx
        mov rdx, rcx
        ret

    .global swapstack_cont
    # a continuation which restores callee-saves and returns
    swapstack_cont:
    swapstack_cont_local:
        pop r15
        pop r14
        pop r13
        pop r12
        pop rbx

        pop rbp
        ret 

    .global begin_resume
    begin_resume:
        mov rdi, rax
        mov rsi, rdx
        ret 
    "
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CalleeSaves {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbx: u64,
    pub rbp: u64,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct StackTop {
    pub ss_cont: usize,
    pub callee_saves: CalleeSaves,
    pub ret: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct ROPFrame {
    pub func_addr: usize,
    pub saved_ret_addr: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct InitialStackTop {
    pub ss_top: StackTop,
    pub rop_frame: ROPFrame,
}
