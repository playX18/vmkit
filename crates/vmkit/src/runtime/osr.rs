//! On-Stack-Replacement support
//!
//! Implementation based on [Hop, Skip, & Jump: Practical On-Stack Replacement for a Cross-Platform Language-Neutral VM](https://www.steveblackburn.org/pubs/papers/osr-vee-2018.pdf)

use super::backtrace::ucontext_from_stack;
use super::backtrace::unwind_sys::{self, *};
use super::stack::{begin_resume, begin_resume2, stack_swap_cont, ROPFrame, Stack, StackTop};
use mmtk::util::Address;

use std::mem::MaybeUninit;
pub struct FrameCursor<'a> {
    unw_cursor: unwind_sys::unw_cursor_t,
    stack: &'a mut Stack,
}

impl<'a> FrameCursor<'a> {
    pub fn new(stack: &'a mut Stack) -> Self {
        unsafe {
            let mut ctx = ucontext_from_stack(stack);
            let mut cursor = MaybeUninit::uninit();

            unw_init_local(cursor.as_mut_ptr(), &mut ctx);
            Self {
                stack,
                unw_cursor: cursor.assume_init(),
            }
        }
    }

    pub fn next_frame(&mut self) {
        let pc = self.pc();
        unsafe {
            if pc.as_usize() == begin_resume as usize {
                println!("lol");
                let sp = self.sp();
                let retaddr = (sp + 8usize).load::<Address>();
                let new_sp = sp + 16usize;

                self.set_pc(retaddr);
                self.set_sp(new_sp);
            } else {
                unwind_sys::unw_step(&mut self.unw_cursor);
            }
        }
    }

    unsafe fn reconstruct_ss_top(&mut self) {
        let ss_top = self.stack.push::<StackTop>().as_mut_ref::<StackTop>();

        ss_top.ss_cont = stack_swap_cont as _;

        ss_top.r15 = self.get_reg(UNW_X86_64_R15) as _;
        ss_top.r14 = self.get_reg(UNW_X86_64_R14) as _;
        ss_top.r13 = self.get_reg(UNW_X86_64_R13) as _;
        ss_top.r12 = self.get_reg(UNW_X86_64_R12) as _;
        ss_top.rbx = self.get_reg(UNW_X86_64_RBX) as _;
        ss_top.ret_addr = self.get_reg(UNW_REG_IP) as _;
    }

    pub unsafe fn pop_frames_to(&mut self) {
        let _pc = self.pc();
        let sp = self.sp();

        self.stack.set_sp(sp);
        self.reconstruct_ss_top();
    }

    pub unsafe fn push_frame(&mut self, func: usize) {
        self.push_frame_impl(func, begin_resume as usize);
    }

    unsafe fn push_frame_impl(&mut self, func: usize, adapter: usize) {
        let pc = self.pc();
        let sp = self.sp();

        // remove the swap-stack top on the physical stack
        self.stack.set_sp(sp);

        let rop_frame = self.stack.push::<ROPFrame>().as_mut_ref::<ROPFrame>();
        // The current return address is saved in the ROP frame so that after
        // `func` returns, we resume from the current PC.
        println!(
            "saved ret addr {} at {:p} (sp: {:p})",
            pc, &rop_frame.saved_ret_addr, rop_frame
        );
        rop_frame.saved_ret_addr = pc.as_usize();
        // the function address
        rop_frame.func_addr = func;

        // The ROP frame should be entered from the adapter, not `func`.
        // Therefore we set the current PC to the adapter.
        self.set_reg(UNW_REG_IP, begin_resume2 as usize as _);
        // The SP has changed because we just pushed a ROP frame.
        self.set_reg(UNW_REG_SP, self.stack.sp().as_usize() as _);
        // Reconstruct the swap-stack top so that the stack top always has the
        // swap-stack resumption protocol.
        self.reconstruct_ss_top();
    }

    pub fn proc_name(&mut self) -> [i8; 256] {
        unsafe {
            let mut buff = [0; 256];
            let mut off = 0;
            unw_get_proc_name(&mut self.unw_cursor, buff.as_mut_ptr(), 256, &mut off);
            buff
        }
    }

    pub fn pc(&self) -> Address {
        unsafe {
            let mut ip = 0;
            unwind_sys::unw_get_reg(
                &self.unw_cursor as *const _ as _,
                unwind_sys::UNW_REG_IP as _,
                &mut ip,
            );
            Address::from_usize(ip as _)
        }
    }

    pub fn sp(&self) -> Address {
        unsafe {
            let mut sp = 0;
            unwind_sys::unw_get_reg(
                &self.unw_cursor as *const _ as _,
                unwind_sys::UNW_REG_SP as _,
                &mut sp,
            );
            Address::from_usize(sp as _)
        }
    }

    pub fn set_pc(&mut self, new_pc: Address) {
        self.set_reg(unwind_sys::UNW_REG_IP, new_pc.as_usize() as _);
    }

    pub fn set_sp(&mut self, new_sp: Address) {
        self.set_reg(unwind_sys::UNW_REG_SP, new_sp.as_usize() as _);
    }
    pub fn get_reg(&mut self, reg: u32) -> u64 {
        unsafe {
            let mut val = 0;
            unwind_sys::unw_get_reg(&self.unw_cursor as *const _ as _, reg as _, &mut val);
            val
        }
    }

    pub fn set_reg(&mut self, reg: u32, value: u64) {
        unsafe {
            unwind_sys::unw_set_reg(&mut self.unw_cursor, reg as _, value);
        }
    }
}
