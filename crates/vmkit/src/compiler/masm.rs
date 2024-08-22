use std::mem::offset_of;

use crate::mm::{vmkit_write_barrier_post, vmkit_write_barrier_post_slow};
use crate::{define_flag, Runtime, ThreadOf};
use crate::{mm::tlab::TLAB, runtime::threads::*};
use macroassembler::assembler::abstract_macro_assembler::{
    AbsoluteAddress, BaseIndex, Extend, Scale,
};
use macroassembler::assembler::{
    abstract_macro_assembler::{Address, JumpList},
    x86assembler::INVALID_GPR,
    RelationalCondition, TargetMacroAssembler,
};
use macroassembler::jit::gpr_info::{ARGUMENT_GPR0, ARGUMENT_GPR1, ARGUMENT_GPR2};
use mmtk::util::alloc::AllocatorSelector;
use mmtk::util::metadata::side_metadata::GLOBAL_SIDE_METADATA_BASE_ADDRESS;

define_flag!(
    bool,
    masm_enable_tlab_alloc,
    true,
    "Enable TLAB fast-path in MacroAssembler"
);

define_flag!(
    bool,
    masm_enable_write_barrier,
    true,
    "Enable Write-Barrier code in MacroAssembler"
);

/// A various set of methods to help in emitting VM code: write barriers, allocation, yieldpoints check
/// etc.
pub trait VMKitMacroAssembler<R: Runtime> {
    fn tlab_allocate(
        &mut self,
        thread: u8,
        obj: u8,
        var_size_in_bytes: u8,
        con_size_in_bytes: usize,
        t1: u8,
        slowpaths: &mut JumpList,
    );

    fn object_reference_write_post(&mut self, dst: Address, val: u8, tmp1: u8, tmp2: u8) {
        let _ = dst;
        let _ = val;
        let _ = tmp1;
        let _ = tmp2;
        unimplemented!()
    }
}

impl<R: Runtime> VMKitMacroAssembler<R> for TargetMacroAssembler {
    fn tlab_allocate(
        &mut self,
        thread: u8,
        obj: u8,
        var_size_in_bytes: u8,
        con_size_in_bytes: usize,
        t1: u8,
        slowpaths: &mut JumpList,
    ) {
        if !masm_enable_tlab_alloc() || ThreadOf::<R>::TLS_OFFSET.is_none() {
            slowpaths.push(self.jump());
            return;
        }

        let tls_offset = ThreadOf::<R>::TLS_OFFSET.unwrap();

        let vmkit = R::vmkit();
        let selector = mmtk::memory_manager::get_allocator_mapping(
            &vmkit.mmtk,
            mmtk::AllocationSemantics::Default,
        );

        let max_non_los_bytes = vmkit
            .mmtk
            .get_plan()
            .constraints()
            .max_non_los_default_alloc_bytes;

        match selector {
            AllocatorSelector::BumpPointer(_) | AllocatorSelector::Immix(_) => (),
            _ => {
                slowpaths.push(self.jump());
                return;
            }
        }

        if var_size_in_bytes == INVALID_GPR {
            if con_size_in_bytes > max_non_los_bytes {
                slowpaths.push(self.jump());
                return;
            }
        } else {
            let j = self.branch64(
                RelationalCondition::AboveOrEqual,
                var_size_in_bytes,
                max_non_los_bytes,
            );
            slowpaths.push(j);
        }

        let tlab_offset = tls_offset + offset_of!(TLSData<R>, tlab);
        let cursor_offset = tlab_offset + TLAB::<R>::CURSOR_OFFSET;
        let end_offset = tlab_offset + TLAB::<R>::END_OFFSET;

        let cursor = Address::new(thread, cursor_offset as i32);
        let end = Address::new(thread, end_offset as i32);

        self.load64(cursor, obj);
        if var_size_in_bytes == INVALID_GPR {
            self.sub64(con_size_in_bytes as i32, obj);
        } else {
            self.sub64(var_size_in_bytes, obj);
        }

        let underflow = self.branch64(RelationalCondition::Below, obj, end);
        slowpaths.push(underflow);
        self.store64(obj, cursor);
    }

    fn object_reference_write_post(&mut self, dst: Address, val: u8, tmp1: u8, tmp2: u8) {
        // skip write barrier code if current GC plan is not generaitonal
        if R::vmkit().mmtk.get_plan().generational().is_none() {
            return;
        }
        let obj = dst.base;
        if *masm_enable_write_barrier() {
            let tmp3 = TargetMacroAssembler::SCRATCH_REGISTER;

            self.mov(obj, tmp2);
            self.rshift64(6i32, tmp2);
            self.mov(GLOBAL_SIDE_METADATA_BASE_ADDRESS.as_usize() as i64, tmp1);
            self.load8(
                BaseIndex::new(tmp1, tmp2, Scale::TimesOne, 0, Extend::None),
                tmp2,
            );

            self.mov(obj, tmp3);
            self.rshift64(3i32, tmp3);
            self.and64(7i32, tmp3);

            self.rshift64(tmp3, tmp2);
            self.and64(tmp2, 1);
            let done = self.branch64(RelationalCondition::NotEqual, tmp2, 1i32);

            self.mov(obj, ARGUMENT_GPR0);
            self.lea64(dst, ARGUMENT_GPR1);
            if val == INVALID_GPR {
                self.xor64(ARGUMENT_GPR2, ARGUMENT_GPR2);
            } else {
                self.mov(val, ARGUMENT_GPR2);
            }

            self.call_op(Some(AbsoluteAddress::new(
                vmkit_write_barrier_post_slow::<R> as _,
            )));

            done.link(self);
        } else {
            self.mov(obj, ARGUMENT_GPR0);
            self.lea64(dst, ARGUMENT_GPR1);
            if val == INVALID_GPR {
                self.xor64(ARGUMENT_GPR2, ARGUMENT_GPR2);
            } else {
                self.mov(val, ARGUMENT_GPR2);
            }

            self.call_op(Some(AbsoluteAddress::new(
                vmkit_write_barrier_post::<R> as _,
            )));
        }
    }
}
