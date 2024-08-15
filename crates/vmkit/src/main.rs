use std::ptr::addr_of_mut;

use vmkit::runtime::{
    osr::FrameCursor,
    stack::{stack_swap, Stack},
};

#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

static mut MAIN_STACK: Stack = Stack::uninit();
static mut CORO_STACK: Stack = Stack::uninit();

extern "C" fn main_stack() -> &'static mut Stack {
    unsafe { &mut *addr_of_mut!(MAIN_STACK) }
}

extern "C" fn coro_stack() -> &'static mut Stack {
    unsafe { &mut *addr_of_mut!(CORO_STACK) }
}

extern "C" fn wow(param: usize) -> usize {
    println!("Hi! I am wow, you have never called me, haven't you?");
    println!("But I am still here. param = {}", param);

    return param * 3;
}

extern "C" fn hxx(m: usize) -> usize {
    unsafe {
        stack_swap(&mut coro_stack(), &mut main_stack(), m);
    }
    m * 2
}

extern "C" fn g(n: usize) -> usize {
    let result = hxx(n * 10);
    println!("the result is {}", result);

    result
}

extern "C" fn f(arg: usize) -> usize {
    let result = g(arg + 1);
    println!("The result is {}", result);
    unsafe { stack_swap(coro_stack(), main_stack(), result) }
}
fn main() {
    *coro_stack() = Stack::new(4096);
    coro_stack().init(f);

    unsafe {
        println!("f: {:p}", f as *const u8);
        println!("h: {:p}", hxx as *const u8);
        println!("wow: {:p}", wow as *const u8);
        let result = stack_swap(main_stack(), coro_stack(), 42);
        println!("Welcome back. Result is {}", result);

        let mut cursor = FrameCursor::new(coro_stack());

        for _ in 0..3 {
            let pc = cursor.pc();

            println!("  pc = {}, sp = {}", pc, cursor.sp());
            println!(
                "  proc = {:?}",
                std::ffi::CStr::from_ptr(&cursor.proc_name()[0])
            );
            cursor.next_frame();
        }

        let arg = std::env::args().collect::<Vec<_>>()[1]
            .parse::<usize>()
            .unwrap();

        println!("{}", arg);

        let mut cursor2 = FrameCursor::new(coro_stack());
        println!("{:?}", std::ffi::CStr::from_ptr(&cursor2.proc_name()[0]));
        cursor2.next_frame();

        cursor2.pop_frames_to();

        for _ in 0..arg {
            cursor2.push_frame(wow as _);
        }

        println!("{:?}", std::ffi::CStr::from_ptr(&cursor2.proc_name()[0]));
        let stack2 = stack_swap(&mut main_stack(), &mut coro_stack(), 1000);

        println!("{}", stack2);
    }
}
