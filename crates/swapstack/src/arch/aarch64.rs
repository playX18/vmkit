pub mod prelude {
    pub use super::*;
}

#[repr(C)]
pub struct StackTop {
    pub ss_cont: usize,
    pub unused: usize,
    pub callee_saves: CalleeSaves,
}

pub struct CalleeSaves {
    pub d8_to_d15: [f64; 15 - 8 + 1],
    pub x19_to_x30: [usize; 30 - 19 + 1],
}

impl StackTop {
    pub fn lr(&self) -> usize {
        self.callee_saves.x19_to_x30[30 - 19]
    }

    pub fn fp(&self) -> usize {
        self.callee_saves.x19_to_x30[29 - 19]
    }

    pub fn ret_addr(&self) -> usize {
        self.lr()
    }

    pub fn set_ret_addr(&mut self, addr: usize) {
        self.callee_saves.x19_to_x30[30 - 19] = addr;
    }

    pub fn set_fp(&mut self, addr: usize) {
        self.callee_saves.x19_to_x30[29 - 19] = addr;
    }
}

#[repr(C)]
pub struct ROPFrame {
    pub func: usize,
    pub ret: usize,
}

#[repr(C)]
pub struct InitialStackTop {
    pub ss_top: StackTop,
    pub rop: ROPFrame,
}
