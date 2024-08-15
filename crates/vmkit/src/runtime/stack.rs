//! Various methods to interact with stack.

use std::{alloc::Layout, arch::global_asm, sync::LazyLock};

use macroassembler::{
    assembler::{link_buffer::LinkBuffer, TargetMacroAssembler},
    jit::{
        gpr_info::{ARGUMENT_GPR0, ARGUMENT_GPR1, ARGUMENT_REGISTERS, CALLEE_SAVE_REGISTERS},
        helpers::AssemblyHelpers,
    },
    wtf::executable_memory_handle::CodeRef,
};

/// thread_start_normal(new_sp: Address, old_sp_loc: Address)
pub static THREAD_START_NORMAL: LazyLock<CodeRef> = LazyLock::new(|| {
    let mut asm = TargetMacroAssembler::new();

    // -- on old stack --
    // C calling convention - enter frame
    asm.emit_function_prologue();

    // save callee saved registers
    for &cs in CALLEE_SAVE_REGISTERS {
        asm.push_to_save(cs);
    }

    // save sp to old_sp_loc
    asm.mov(TargetMacroAssembler::STACK_POINTER_REGISTER, ARGUMENT_GPR1);

    // switch to new stack
    asm.mov(ARGUMENT_GPR0, TargetMacroAssembler::STACK_POINTER_REGISTER);

    // -- on new stack --
    // arguments (reverse order of runtime_load_args)
    for &arg in ARGUMENT_REGISTERS.iter().rev() {
        asm.pop_to_restore(arg);
    }

    // at this point new stack is clean (no intermediate values)

    asm.pop_to_restore(TargetMacroAssembler::FRAME_POINTER_REGISTER);
    asm.pop_to_restore(TargetMacroAssembler::SCRATCH_REGISTER);
    asm.call_op(Some(TargetMacroAssembler::SCRATCH_REGISTER));

    let mut lb = LinkBuffer::from_macro_assembler(&mut asm).unwrap();
    lb.finalize_without_disassembly()
});

// thread_exit(old_sp: Address)
pub static THREAD_EXIT: LazyLock<CodeRef> = LazyLock::new(|| {
    let mut asm = TargetMacroAssembler::new();

    asm.mov(ARGUMENT_GPR0, TargetMacroAssembler::STACK_POINTER_REGISTER);

    for &cs in CALLEE_SAVE_REGISTERS.iter().rev() {
        asm.pop_to_restore(cs);
    }

    asm.pop_to_restore(TargetMacroAssembler::FRAME_POINTER_REGISTER);
    asm.ret();

    let mut lb = LinkBuffer::from_macro_assembler(&mut asm).unwrap();
    lb.finalize_without_disassembly()
});

use mmtk::util::Address as Addr;

#[repr(C)]
pub struct Stack {
    sp: Addr,
    start: Addr,
    size: usize,
}

impl Stack {
    pub fn sp(&self) -> Addr {
        self.sp
    }

    pub unsafe fn set_sp(&mut self, sp: Addr) {
        self.sp = sp;
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn start(&self) -> Addr {
        self.start
    }

    pub const fn uninit() -> Self {
        Self {
            sp: Addr::ZERO,
            size: 0,
            start: Addr::ZERO,
        }
    }

    pub fn new(size: usize) -> Self {
        unsafe {
            let mem = std::alloc::alloc_zeroed(Layout::from_size_align(size, 8).unwrap());
            Self {
                start: Addr::from_mut_ptr(mem),
                size: size,
                sp: Addr::from_mut_ptr(mem) + size,
            }
        }
    }

    pub const fn from_parts(sp: Addr, size: usize, start: Addr) -> Self {
        Self { sp, size, start }
    }

    pub fn top_buffer(&self) -> Addr {
        self.sp
    }

    pub fn move_sp(&mut self, off: isize) {
        self.sp = self.sp.offset(off);
    }

    pub fn push<T>(&mut self) -> Addr {
        self.move_sp(-(size_of::<T>() as isize));
        self.sp
    }

    unsafe fn init_impl(&mut self, func: usize, adapter: usize) {
        let init_top = self.push::<InitialStackTop>();

        let init_top = unsafe { init_top.as_mut_ref::<InitialStackTop>() };

        init_top.ss_top.ss_cont = stack_swap_cont as usize;
        init_top.ss_top.ret_addr = adapter;
        init_top.rop_frame.func_addr = func;
    }

    pub fn init(&mut self, func: extern "C" fn(usize) -> usize) {
        unsafe {
            self.init_impl(func as _, begin_resume as _);
        }
    }
}

cfg_if::cfg_if! {
    if #[cfg(target_arch="x86_64")] {

        #[repr(C, align(16))]
        pub struct StackTop {
            pub ss_cont: usize,
            pub r15: usize,
            pub r14: usize,
            pub r13: usize,
            pub r12: usize,
            pub rbx: usize,
            pub rbp: usize,
            pub ret_addr: usize,
        }

        #[repr(C)]
        pub struct ROPFrame {
            pub func_addr: usize,
            pub saved_ret_addr: usize,
        }

        #[repr(C)]
        pub struct InitialStackTop {
            pub ss_top: StackTop,
            pub rop_frame: ROPFrame
        }

    }
}

global_asm! {
    "
        .global stack_swap
        stack_swap:
            push rbp
            mov rbp, rsp

            push rbx
            push r12
            push r13
            push r14
            push r15

            lea rcx, [rip + {}]
            push rcx
            
            mov [rdi], rsp
            mov rsp, [rsi]
            mov rax, rdx
            ret

        .global stack_swap_cont
        stack_swap_cont:
            pop r15
            pop r14
            pop r13
            pop r12
            pop rbx

            pop rbp
            ret
        .global begin_resume2
        .global begin_resume
        begin_resume2:
            int3
        begin_resume:
            mov rdi, rax
            ret
    ",
    sym stack_swap_cont
}

extern "C" {
    pub fn stack_swap(from: &mut Stack, to: &mut Stack, value: usize) -> usize;
    pub(super) fn stack_swap_cont();
    pub(super) fn begin_resume2();
    pub(super) fn begin_resume();
}
