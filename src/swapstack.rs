use std::sync::LazyLock;

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
