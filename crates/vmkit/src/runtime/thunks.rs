//! Runtime thunks
//!
//! Various code that is generated on the first use and is specialized for the VM implementation, one case of this
//! is swapstack.  Some code can be generic enough and not require bound on `Runtime` trait, this code is moved into separate LazyLock.

use std::{
    marker::PhantomData,
    mem::{offset_of, transmute},
    sync::LazyLock,
};

use macroassembler::{
    assembler::{
        abstract_macro_assembler::{AbsoluteAddress, Address},
        link_buffer::LinkBuffer,
        TargetMacroAssembler,
    },
    jit::{
        fpr_info::{ARGUMENT_FPR0, RETURN_VALUE_FPR},
        gpr_info::{
            ARGUMENT_GPR0, ARGUMENT_GPR1, ARGUMENT_GPR2, CALLEE_SAVE_REGISTERS, CS0, CS1, CS2,
            RETURN_VALUE_GPR,
        },
        helpers::AssemblyHelpers,
    },
    wtf::executable_memory_handle::CodeRef,
};

use crate::{
    runtime::threads::{stack::Stack, vmkit_get_tls, TLSData},
    Runtime,
};

macro_rules! wrap_thunk {
    ($thunk: ident = unsafe fn $name: ident ($($arg: ident : $t: ty),*) -> $r: ty) => {
        pub unsafe extern "C" fn $name($($arg: $t),*) -> $r {
            let func: extern "C" fn($($t),*) -> $r = std::mem::transmute($thunk.start());

            func($($arg),*)
        }
    };

    ($thunk: ident = fn $name: ident ($($arg: ident : $t: ty),*) -> $r: ty) => {
        pub extern "C" fn $name($($arg: $t),*) -> $r {
            unsafe {
                let func: extern "C" fn($($t),*) -> $r = std::mem::transmute($thunk.start());

                func($($arg),*)
            }
        }
    };
}

fn finalize(asm: &mut TargetMacroAssembler, format: &str) -> CodeRef {
    let mut lb = LinkBuffer::from_macro_assembler(asm).unwrap();
    if log::log_enabled!(log::Level::Debug) {
        let mut out = String::new();
        let code = lb
            .finalize_with_disassembly(true, format, &mut out)
            .unwrap();
        log::debug!(target: "vmkit::thunks", "{}", out);
        code
    } else {
        lb.finalize_without_disassembly()
    }
}

pub unsafe fn thread_exit<R: Runtime>(arg: usize) -> ! {
    let tls = vmkit_get_tls::<R>();
    let native = tls.native_sp.get();
    println!("exit to {:p}", native);
    swapstack2::<R>(native, tls.stack.get(), arg);
    std::hint::unreachable_unchecked()
}

fn generate_swapstack<R: Runtime, const HAS_OLD_STACK: bool>() -> CodeRef {
    let mut asm = TargetMacroAssembler::new();

    let scratch = TargetMacroAssembler::SCRATCH_REGISTER;

    asm.emit_function_prologue();

    for &cs in CALLEE_SAVE_REGISTERS {
        asm.push_to_save(cs);
    }

    asm.mov(SWAPSTACK_CONT.start() as i64, scratch);
    asm.push_to_save(scratch);

    asm.mov(ARGUMENT_GPR0, CS0);
    asm.mov(ARGUMENT_GPR1, CS1);
    if HAS_OLD_STACK {
        asm.mov(ARGUMENT_GPR2, CS2);
    }

    {
        asm.call_op(Some(AbsoluteAddress::new(vmkit_get_tls::<R> as *const u8)));
        asm.mov(RETURN_VALUE_GPR, scratch);
        // save old SP
        if !HAS_OLD_STACK {
            // load the current stackref
            asm.load64(
                Address::new(scratch, offset_of!(TLSData::<R>, stack) as i32),
                RETURN_VALUE_GPR,
            );
            asm.store64(
                TargetMacroAssembler::STACK_POINTER_REGISTER,
                Address::new(RETURN_VALUE_GPR, Stack::SP_OFFSET as i32),
            );
            asm.store64(
                RETURN_VALUE_GPR,
                Address::new(CS0, Stack::LINK_OFFSET as i32),
            );
        } else {
            asm.store64(
                TargetMacroAssembler::STACK_POINTER_REGISTER,
                Address::new(CS1, Stack::SP_OFFSET as i32),
            );
            asm.store64(CS1, Address::new(CS0, Stack::LINK_OFFSET as i32));
        }

        // store the new stackref
        asm.store64(
            CS0,
            Address::new(scratch, offset_of!(TLSData<R>, stack) as i32),
        );
    }

    // Load the new sp from the swapee
    asm.load64(
        Address::new(CS0, Stack::SP_OFFSET as i32),
        TargetMacroAssembler::STACK_POINTER_REGISTER,
    );

    let arg = if HAS_OLD_STACK { CS2 } else { CS1 };
    // move argument to return value
    asm.mov(arg, RETURN_VALUE_GPR);
    asm.ret();
    finalize(&mut asm, "swapstack(*mut Stack, usize) -> usize")
}

fn generate_thread_start<R: Runtime>() -> CodeRef {
    let mut asm = TargetMacroAssembler::new();

    let scratch = TargetMacroAssembler::SCRATCH_REGISTER;

    asm.emit_function_prologue();

    for &cs in CALLEE_SAVE_REGISTERS {
        asm.push_to_save(cs);
    }

    asm.mov(SWAPSTACK_CONT.start() as i64, scratch);
    asm.push_to_save(scratch);
    asm.mov(ARGUMENT_GPR1, CS1);
    asm.mov(ARGUMENT_GPR0, CS0);
    {
        asm.call_op(Some(AbsoluteAddress::new(vmkit_get_tls::<R> as *const u8)));
        asm.mov(RETURN_VALUE_GPR, scratch);

        // load the current stackref
        asm.load64(
            Address::new(scratch, offset_of!(TLSData::<R>, native_sp) as i32),
            RETURN_VALUE_GPR,
        );

        // store the new stackref
        asm.store64(
            CS0,
            Address::new(scratch, offset_of!(TLSData<R>, stack) as i32),
        );
        // save old SP
        asm.store64(
            TargetMacroAssembler::STACK_POINTER_REGISTER,
            Address::new(RETURN_VALUE_GPR, Stack::SP_OFFSET as i32),
        );
        asm.store64(
            CS0,
            Address::new(RETURN_VALUE_GPR, Stack::LINK_OFFSET as i32),
        );
    }
    // Load the new sp from the swapee
    asm.load64(
        Address::new(CS0, Stack::SP_OFFSET as i32),
        TargetMacroAssembler::STACK_POINTER_REGISTER,
    );

    // move argument to return value
    asm.mov(CS1, RETURN_VALUE_GPR);
    asm.ret();
    finalize(&mut asm, "swapstack(*mut Stack, usize) -> usize")
}

pub static SWAPSTACK_CONT: LazyLock<CodeRef> = LazyLock::new(|| {
    let mut asm = TargetMacroAssembler::new();

    for &cs in CALLEE_SAVE_REGISTERS.iter().rev() {
        asm.pop_to_restore(cs);
    }

    asm.emit_function_epilogue_with_empty_frame();
    asm.ret();

    finalize(&mut asm, "swapstack_cont()")
});

pub static BEGIN_RESUME: LazyLock<CodeRef> = LazyLock::new(|| {
    let mut asm = TargetMacroAssembler::new();

    asm.move_double(RETURN_VALUE_FPR, ARGUMENT_FPR0);
    asm.mov(RETURN_VALUE_GPR, ARGUMENT_GPR0);
    asm.ret();
    finalize(&mut asm, "begin_resume")
});

pub struct Thunks<R: Runtime> {
    pub swapstack: LazyLock<CodeRef>,
    pub swapstack2: LazyLock<CodeRef>,
    pub thread_start: LazyLock<CodeRef>,
    marker: PhantomData<R>,
}

impl<R: Runtime> Thunks<R> {
    pub fn new() -> Self {
        Self {
            swapstack: LazyLock::new(generate_swapstack::<R, false>),
            swapstack2: LazyLock::new(generate_swapstack::<R, true>),
            thread_start: LazyLock::new(generate_thread_start::<R>),
            marker: PhantomData,
        }
    }
}

pub unsafe fn swapstack<R: Runtime>(stackref: *mut Stack, arg: usize) -> usize {
    let func: extern "C" fn(*mut Stack, usize) -> usize =
        transmute(R::vmkit().thunks.swapstack.start());

    func(stackref, arg)
}

pub unsafe fn swapstack2<R: Runtime>(
    stackref: *mut Stack,
    old_stackref: *mut Stack,
    arg: usize,
) -> usize {
    let func: extern "C" fn(*mut Stack, *mut Stack, usize) -> usize =
        transmute(R::vmkit().thunks.swapstack2.start());

    func(stackref, old_stackref, arg)
}

pub unsafe fn thread_start<R: Runtime>(stackref: *mut Stack, arg: usize) -> usize {
    let func: extern "C" fn(*mut Stack, usize) -> usize =
        transmute(R::vmkit().thunks.thread_start.start());

    func(stackref, arg)
}

// save_current_stack(stackref: *mut Stack, callee_saves: *mut usize, f: fn())
static SAVE_CURRENT_STACK: LazyLock<CodeRef> = LazyLock::new(|| {
    let mut asm = TargetMacroAssembler::new();

    asm.store64(
        TargetMacroAssembler::FRAME_POINTER_REGISTER,
        Address::new(ARGUMENT_GPR0, Stack::BP_OFFSET as i32),
    );
    asm.store64(
        TargetMacroAssembler::STACK_POINTER_REGISTER,
        Address::new(ARGUMENT_GPR0, Stack::SP_OFFSET as i32),
    );

    asm.load64(
        Address::new(TargetMacroAssembler::STACK_POINTER_REGISTER, 0),
        RETURN_VALUE_GPR,
    );
    asm.store64(
        RETURN_VALUE_GPR,
        Address::new(ARGUMENT_GPR0, Stack::IP_OFFSET as i32),
    );

    let mut off = 0i32;

    for &cs in CALLEE_SAVE_REGISTERS.iter() {
        asm.store64(cs, Address::new(ARGUMENT_GPR1, off));
        off += size_of::<usize>() as i32;
    }

    asm.call_op(Some(ARGUMENT_GPR2));
    asm.ret();

    finalize(&mut asm, "save_current_stack")
});

wrap_thunk!(SAVE_CURRENT_STACK = unsafe fn save_current_stack(stackref: *mut Stack, callee_saves: *mut usize, callback: extern "C" fn()) -> ());
