use std::mem::MaybeUninit;

use mmtk::util::Address;

use super::stack::{Stack, StackTop};

#[derive(Debug)]
pub struct Frame {
    pub fp: Address,
    pub pc: Address,
}

impl Frame {
    pub fn fp(&self) -> Address {
        self.fp
    }

    pub fn pc(&self) -> Address {
        self.pc
    }
}

pub struct Backtrace(Vec<Frame>);

impl Backtrace {
    pub const fn empty() -> Self {
        Self(Vec::new())
    }
}

#[allow(non_snake_case, non_upper_case_globals, non_camel_case_types)]
pub mod unwind_sys {
    use std::{
        ffi::{c_char, c_int},
        os::raw::c_void,
    };

    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

    extern "C" {
        #[link_name = "_Ux86_64_getcontext"]
        pub fn unw_tdep_getcontext(ctx: *mut unw_tdep_context_t) -> c_int;

        #[link_name = "_Ux86_64_init_local"]
        pub fn unw_init_local(cur: *mut unw_cursor_t, ctx: *mut unw_context_t) -> c_int;

        #[link_name = "_Ux86_64_init_remote"]
        pub fn unw_init_remote(
            cur: *mut unw_cursor_t,
            spc: unw_addr_space_t,
            p: *mut c_void,
        ) -> c_int;

        #[link_name = "_Ux86_64_step"]
        pub fn unw_step(cur: *mut unw_cursor_t) -> c_int;

        #[link_name = "_Ux86_64_get_reg"]
        pub fn unw_get_reg(
            cur: *mut unw_cursor_t,
            reg: unw_regnum_t,
            valp: *mut unw_word_t,
        ) -> c_int;

        #[link_name = "_Ux86_64_set_reg"]
        pub fn unw_set_reg(cur: *mut unw_cursor_t, reg: unw_regnum_t, val: unw_word_t) -> c_int;

        #[link_name = "_Ux86_64_resume"]
        pub fn unw_resume(cur: *mut unw_cursor_t) -> c_int;

        #[link_name = "_Ux86_64_create_addr_space"]
        pub fn unw_create_addr_space(
            accessors: *mut unw_accessors_t,
            byteorder: c_int,
        ) -> unw_addr_space_t;

        #[link_name = "_Ux86_64_destroy_addr_space"]
        pub fn unw_destroy_addr_space(spc: unw_addr_space_t);

        #[link_name = "_Ux86_64_get_accessors"]
        pub fn unw_get_accessors(spc: unw_addr_space_t) -> *mut unw_accessors_t;

        #[link_name = "_Ux86_64_flush_cache"]
        pub fn unw_flush_cache(spc: unw_addr_space_t, lo: unw_word_t, hi: unw_word_t);

        #[link_name = "_Ux86_64_set_caching_policy"]
        pub fn unw_set_caching_policy(spc: unw_addr_space_t, policy: unw_caching_policy_t)
            -> c_int;

        #[link_name = "_Ux86_64_regname"]
        pub fn unw_regname(reg: unw_regnum_t) -> *const c_char;

        #[link_name = "_Ux86_64_get_proc_info"]
        pub fn unw_get_proc_info(cur: *mut unw_cursor_t, info: *mut unw_proc_info_t) -> c_int;

        #[link_name = "_Ux86_64_get_save_loc"]
        pub fn unw_get_save_loc(cur: *mut unw_cursor_t, a: c_int, p: *mut unw_save_loc_t) -> c_int;

        #[link_name = "_Ux86_64_is_fpreg"]
        pub fn unw_tdep_is_fpreg(reg: unw_regnum_t) -> c_int;

        #[link_name = "_Ux86_64_is_signal_frame"]
        pub fn unw_is_signal_frame(cur: *mut unw_cursor_t) -> c_int;

        #[link_name = "_Ux86_64_get_proc_name"]
        pub fn unw_get_proc_name(
            cur: *mut unw_cursor_t,
            buf: *mut c_char,
            len: usize,
            offp: *mut unw_word_t,
        ) -> c_int;

        #[link_name = "_Ux86_64_strerror"]
        pub fn unw_strerror(err_code: c_int) -> *const c_char;

        #[link_name = "_Ux86_64_local_addr_space"]
        pub static unw_local_addr_space: unw_addr_space_t;
    }
}
use unwind_sys::*;
pub unsafe fn ucontext_from_stack(stack: &Stack) -> ucontext_t {
    let mut ctx: ucontext_t = MaybeUninit::zeroed().assume_init();

    ctx.uc_stack.ss_sp = stack.sp().as_usize() as _;
    ctx.uc_stack.ss_size = stack.size();
    ctx.uc_stack.ss_flags = 0;

    let ss_top = stack.sp().as_mut_ref::<StackTop>();

    ctx.uc_mcontext.gregs[REG_R15 as usize] = ss_top.r15 as _;
    ctx.uc_mcontext.gregs[REG_R14 as usize] = ss_top.r14 as _;
    ctx.uc_mcontext.gregs[REG_R13 as usize] = ss_top.r13 as _;
    ctx.uc_mcontext.gregs[REG_R12 as usize] = ss_top.r12 as _;
    ctx.uc_mcontext.gregs[REG_RBX as usize] = ss_top.rbx as _;
    ctx.uc_mcontext.gregs[REG_RIP as usize] = ss_top.ret_addr as _;
    ctx.uc_mcontext.gregs[REG_RSP as usize] = (ss_top as *const StackTop).add(1) as _;

    ctx
}

macro_rules! cenum {
    ($name: ident, $($rest:tt)*)=> {
        pub const $name: usize = 0;
        cenum!(@c 1; $($rest)*);
    };

    (@c $c: expr; $name: ident, $($rest:tt)*) => {
        pub const $name: usize = $c;
        cenum!(@c $c+1; $($rest)*);
    };
    (@c $c: expr; $name: ident) => {
        pub const $name: usize = $c;
    }
}

cenum! {
    REG_R8,
    //  REG_R8		REG_R8
      REG_R9,
    //  REG_R9		REG_R9
      REG_R10,
    //  REG_R10	REG_R10
      REG_R11,
    //  REG_R11	REG_R11
      REG_R12,
    //  REG_R12	REG_R12
      REG_R13,
    //  REG_R13	REG_R13
      REG_R14,
    //  REG_R14	REG_R14
      REG_R15,
    //  REG_R15	REG_R15
      REG_RDI,
    //  REG_RDI	REG_RDI
      REG_RSI,
    //  REG_RSI	REG_RSI
      REG_RBP,
    //  REG_RBP	REG_RBP
      REG_RBX,
    //  REG_RBX	REG_RBX
      REG_RDX,
    //  REG_RDX	REG_RDX
      REG_RAX,
    //  REG_RAX	REG_RAX
      REG_RCX,
    //  REG_RCX	REG_RCX
      REG_RSP,
    //  REG_RSP	REG_RSP
      REG_RIP,
    //  REG_RIP	REG_RIP
      REG_EFL,
    //  REG_EFL	REG_EFL
      REG_CSGSFS,		/* Actually short cs, gs, fs, __pad0.  */
    //  REG_CSGSFS	REG_CSGSFS
      REG_ERR,
    //  REG_ERR	REG_ERR
      REG_TRAPNO,
    //  REG_TRAPNO	REG_TRAPNO
      REG_OLDMASK,
    //  REG_OLDMASK	REG_OLDMASK
      REG_CR2
    //  REG_CR2	REG_CR2
}
