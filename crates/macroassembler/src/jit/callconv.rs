use jit_allocator::util::align_up;
use num::integer::lcm;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CallConvResult {
    GPR(u8),
    GPREX(u8, u8),
    FPR(u8),
    STACK,
}

/// A list of types supported for calls. Note: we can support SystemV ABI where two fields of structs
/// are passed in registers but this is too complicated so we just allow ints, floats and pointers
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Type {
    Int32,
    Int64,
    Float32,
    Float64,
    Ptr,
}

impl Type {
    pub fn align(&self) -> usize {
        match self {
            Self::Int32 => align_of::<i32>(),
            Self::Int64 => align_of::<i64>(),
            Self::Float32 => align_of::<f32>(),
            Self::Float64 => align_of::<f64>(),
            Self::Ptr => align_of::<*const ()>(),
        }
    }

    pub fn size(&self) -> usize {
        match self {
            Self::Int32 => size_of::<i32>(),
            Self::Int64 => size_of::<i64>(),
            Self::Float32 => size_of::<f32>(),
            Self::Float64 => size_of::<f64>(),
            Self::Ptr => size_of::<*const ()>(),
        }
    }
}

/// C Calling convention
pub mod c {
    use crate::jit::{
        fpr_info::{FP_ARGUMENT_REGISTERS, RETURN_VALUE_FPRS},
        gpr_info::{ARGUMENT_REGISTERS, RETURN_VALUE_REGISTERS},
    };

    use super::*;
    pub fn compute_arguments(tys: &[Type]) -> Vec<CallConvResult> {
        let mut ret = vec![];
        let mut gprc = 0;
        let mut fprc = 0;

        for ty in tys.iter() {
            match ty {
                Type::Int32 | Type::Int64 | Type::Ptr => {
                    if gprc < ARGUMENT_REGISTERS.len() {
                        let arg_gpr = ARGUMENT_REGISTERS[gprc];
                        ret.push(CallConvResult::GPR(arg_gpr));
                        gprc += 1;
                    } else {
                        ret.push(CallConvResult::STACK);
                    }
                }
                _ => {
                    if fprc < FP_ARGUMENT_REGISTERS.len() {
                        let arg_fpr = FP_ARGUMENT_REGISTERS[fprc];
                        ret.push(CallConvResult::FPR(arg_fpr));
                        fprc += 1;
                    } else {
                        ret.push(CallConvResult::STACK);
                    }
                }
            }
        }

        ret
    }

    pub fn compute_return_values(tys: &[Type]) -> Vec<CallConvResult> {
        let mut ret = vec![];
        let mut gprc = 0;
        let mut fprc = 0;

        for ty in tys.iter() {
            match ty {
                Type::Int32 | Type::Int64 | Type::Ptr => {
                    if gprc < RETURN_VALUE_REGISTERS.len() {
                        let arg_gpr = RETURN_VALUE_REGISTERS[gprc];
                        ret.push(CallConvResult::GPR(arg_gpr));
                        gprc += 1;
                    } else {
                        ret.push(CallConvResult::STACK);
                    }
                }
                _ => {
                    if fprc < RETURN_VALUE_FPRS.len() {
                        let arg_fpr = RETURN_VALUE_FPRS[fprc];
                        ret.push(CallConvResult::FPR(arg_fpr));
                        fprc += 1;
                    } else {
                        ret.push(CallConvResult::STACK);
                    }
                }
            }
        }

        ret
    }
    pub fn compute_stack_retvals(tys: &[Type]) -> (usize, Vec<usize>) {
        let callconv = compute_return_values(tys);

        let mut stack_ret_val_tys = vec![];
        for i in 0..callconv.len() {
            let ref cc = callconv[i];
            match cc {
                &CallConvResult::STACK => stack_ret_val_tys.push(tys[i].clone()),
                _ => {}
            }
        }

        compute_stack_locations(&stack_ret_val_tys)
    }

    pub fn compute_stack_args(tys: &[Type]) -> (usize, Vec<usize>) {
        let callconv = compute_arguments(tys);

        let mut stack_arg_tys = vec![];

        for i in 0..callconv.len() {
            match callconv[i] {
                CallConvResult::STACK => stack_arg_tys.push(tys[i]),
                _ => {}
            }
        }

        compute_stack_locations(&stack_arg_tys)
    }

    pub fn compute_stack_locations(stack_val_tys: &[Type]) -> (usize, Vec<usize>) {
        let (stack_arg_size, _, stack_arg_offsets) = sequential_layout(stack_val_tys);

        // "The end of the input argument area shall be aligned on a 16
        // (32, if __m256 is passed on stack) byte boundary." - x86 ABI
        // if we need to special align the args, we do it now
        // (then the args will be put to stack following their regular alignment)
        let mut stack_arg_size_with_padding = stack_arg_size;

        if stack_arg_size % 16 == 0 {
            // do not need to adjust rsp
        } else if stack_arg_size % 8 == 0 {
            // adjust rsp by -8
            stack_arg_size_with_padding += 8;
        } else {
            let rem = stack_arg_size % 16;
            let stack_arg_padding = 16 - rem;
            stack_arg_size_with_padding += stack_arg_padding;
        }

        (stack_arg_size_with_padding, stack_arg_offsets)
    }
}

pub fn sequential_layout(tys: &[Type]) -> (usize, usize, Vec<usize>) {
    let mut offsets = vec![];
    let mut cur = 0;
    let mut struct_align = 1;

    for &ty in tys {
        struct_align = lcm(struct_align, ty.align());
        cur = align_up(cur, ty.align());
        offsets.push(cur);
        cur += ty.size();
    }

    let size = align_up(cur, struct_align);

    (size, struct_align, offsets)
}
