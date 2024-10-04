use cfg_if::cfg_if;

pub type FContext = *mut ();

#[repr(C)]
#[derive(PartialEq, Eq)]
pub struct Transfer {
    pub fctx: FContext,
    pub data: *mut (),
}

extern "C" {
    pub fn jump_fcontext(to: FContext, vp: *mut u8) -> Transfer;
    pub fn make_fcontext(sp: *mut u8, size: usize, fun: extern "C-unwind" fn(Transfer))
        -> FContext;
    pub fn ontop_fcontext(
        to: FContext,
        vp: *mut u8,
        fun: extern "C-unwind" fn(Transfer) -> Transfer,
    ) -> Transfer;
}

pub mod x86_64 {

    #[derive(Clone, Copy)]
    #[repr(C)]
    #[cfg(all(not(target_vendor = "apple"), not(windows)))]
    pub struct FContextTop {
        /// shadow stack or TLS guard record here
        pub unused: [u64; 2],
        pub r12: u64,
        pub r13: u64,
        pub r14: u64,
        pub r15: u64,
        pub rbx: u64,
        pub rbp: u64,
        pub ret_addr: u64,
    }

    #[cfg(all(not(target_vendor = "apple"), not(windows)))]
    const _: () = {
        use std::mem::offset_of;

        assert!(offset_of!(FContextTop, rbp) == 0x38);
    };

    #[derive(Clone, Copy)]
    #[repr(C)]
    pub struct FContextTopApple {
        pub unused: [u64; 1],
        pub r12: u64,
        pub r13: u64,
        pub r14: u64,
        pub r15: u64,
        pub rbx: u64,
        pub rbp: u64,
    }

    const _: () = {
        use std::mem::offset_of;

        assert!(offset_of!(FContextTopApple, rbp) == 0x30);
    };

    #[derive(Copy, Clone)]
    #[repr(C)]
    pub struct FContextTopWindows {
        pub xmm6: u128,
        pub xmm7: u128,
        pub xmm8: u128,
        pub xmm9: u128,
        pub xmm10: u128,
        pub xmm11: u128,
        pub xmm12: u128,
        pub xmm13: u128,
        pub xmm14: u128,
        pub xmm15: u128,
        pub mmx_and_x87: u64,
        pub align: u64,
        pub fiber_local_storage: u64,
        pub deallocation_stack: u64,
        pub stack_limit: u64,
        pub stack_base: u64,

        pub r12: u64,
        pub r13: u64,
        pub r14: u64,
        pub r15: u64,
        pub rdi: u64,
        pub rsi: u64,
        pub rbx: u64,
        pub rbp: u64,
        pub transfer_addr: u64,
        pub ret_addr: u64,
    }

    const _: () = {
        use std::mem::offset_of;

        assert!(offset_of!(FContextTopWindows, rbp) == 0x108);
    };
}

pub mod aarch64 {
    #[derive(Clone, Copy)]
    #[repr(C)]
    pub struct FContextTop {
        pub d8: u64,
        pub d9: u64,
        pub d10: u64,
        pub d11: u64,
        pub d12: u64,
        pub d13: u64,
        pub d14: u64,
        pub d15: u64,
        pub x19: u64,
        pub x20: u64,
        pub x21: u64,
        pub x22: u64,
        pub x23: u64,
        pub x24: u64,
        pub x25: u64,
        pub x26: u64,
        pub x27: u64,
        pub x28: u64,
        pub fp: u64,
        pub lr: u64,
        pub pc: u64,
        pub align: u64,
    }

    #[derive(Clone, Copy)]
    #[repr(C)]
    pub struct FContextTopWindows {
        pub d8: u64,
        pub d9: u64,
        pub d10: u64,
        pub d11: u64,
        pub d12: u64,
        pub d13: u64,
        pub d14: u64,
        pub d15: u64,
        pub x19: u64,
        pub x20: u64,
        pub x21: u64,
        pub x22: u64,
        pub x23: u64,
        pub x24: u64,
        pub x25: u64,
        pub x26: u64,
        pub x27: u64,
        pub x28: u64,
        pub fp: u64,
        pub lr: u64,
        pub fiber_data: u64,
        pub base: u64,
        pub limit: u64,
        pub dealloc: u64,
        pub pc: u64,
        pub align: u64,
    }
}

pub mod riscv64 {
    pub struct FContextTop {
        pub fs0: u64,
        pub fs1: u64,
        pub fs2: u64,
        pub fs3: u64,
        pub fs4: u64,
        pub fs5: u64,
        pub fs6: u64,
        pub fs7: u64,
        pub fs8: u64,
        pub fs9: u64,
        pub fs10: u64,
        pub fs11: u64,
        pub s0: u64,
        pub s1: u64,
        pub s2: u64,
        pub s3: u64,
        pub s4: u64,
        pub s5: u64,
        pub s6: u64,
        pub s7: u64,
        pub s8: u64,
        pub s9: u64,
        pub s10: u64,
        pub s11: u64,
        pub ra: u64,
        pub pc: u64,
    }
}

pub mod x86 {
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct FContextTop {
        pub fc_mxcsr: u32,
        pub fc_x87_cw: u32,
        pub guard: u32,
        pub edi: u32,
        pub esi: u32,
        pub ebx: u32,
        pub ebp: u32,
        pub eip: u32,
        pub hidden: u32,
        pub to: u32,
        pub data: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct FContextTopApple {
        pub fc_mxcsr: u32,
        pub fc_x87_cw: u32,
        pub edi: u32,
        pub esi: u32,
        pub ebx: u32,
        pub ebp: u32,
        pub eip: u32,
        pub hidden: u32,
        pub to: u32,
        pub data: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct FContextTopWindows {
        pub fc_mxcsr: u32,
        pub fc_x87_cw: u32,
        pub fc_strg: u32,
        pub fc_dealloc: u32,
        pub limit: u32,
        pub base: u32,
        pub fc_seh: u32,
        pub edi: u32,
        pub esi: u32,
        pub ebx: u32,
        pub ebp: u32,
        pub eip: u32,
        pub to: u32,
        pub data: u32,
        pub eh_nxt: u32,
        pub seh_hndlr: u32,
    }
}

pub mod arm {

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct FContextTop {
        pub s16: u32,
        pub s17: u32,
        pub s18: u32,
        pub s19: u32,
        pub s20: u32,
        pub s21: u32,
        pub s22: u32,
        pub s23: u32,
        pub s24: u32,
        pub s25: u32,
        pub s26: u32,
        pub s27: u32,
        pub s28: u32,
        pub s29: u32,
        pub s30: u32,
        pub s31: u32,
        pub hidden: u32,
        pub v1: u32,
        pub v2: u32,
        pub v3: u32,
        pub v4: u32,
        pub v5: u32,
        pub v6: u32,
        pub v7: u32,
        pub v8: u32,
        pub lr: u32,
        pub pc: u32,
        pub fctx: u32,
        pub data: u32,
    }
}

cfg_if!(
    if #[cfg(target_arch="x86_64")] {
        cfg_if!(if #[cfg(windows)] {
            pub type PlatformFContextTop = x86_64::FContextTopWindows;
        } else if #[cfg(target_vendor="apple")] {
            pub type PlatformFContextTop = x86_64::FContextTopApple;
        } else {
            pub type PlatformFContextTop = x86_64::FContextTop;
        });
    } else if #[cfg(target_arch="aarch64")] {
        cfg_if!(if #[cfg(windows)] {
            pub type PlatformFContextTop = aarch64::FContextTopWindows;
        } else {
            pub type PlatformFContextTop = aarch64::FContextTop;
        });
    } else if #[cfg(target_arch="arm")] {
        pub type PlatformFContextTop = arm::FContextTop;
    } else if #[cfg(target_arch="riscv64")] {
        pub type PlatformFContextTop = riscv64::FContextTop;
    } else if #[cfg(target_arch="x86")] {
        cfg_if!(if #[cfg(windows)] {
            pub type PlatformFContextTop = x86::FContextTopWindows;
        } else if #[cfg(target_vendor="apple")] {
            pub type PlatformFContextTop = x86::FContextTopApple;
        } else {
            pub type PlatformFContextTop = x86::FContextTop;
        });
    } else {
        /* TODO: Error? */
    }
);
