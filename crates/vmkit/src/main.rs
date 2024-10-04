
use vmkit::{
    define_flag,
    mock::{MockThread, MockVM},
    runtime::{
        options::MMTKFlags,
        threads::{TLSData, Thread},
    },
    Runtime,
};

struct A;

struct B;

define_flag!(A => usize, flag, 0, "A flag");
define_flag!(B => usize, flag, 0, "B flag");

fn main() {
    env_logger::init();
    vmkit::utils::flags::parse_with_prefix::<MMTKFlags>("gc", std::env::args(), std::env::vars())
        .unwrap();

    let _vmkit = MockVM::vmkit();

    let _t = MockThread::new(TLSData::new(true));

    let (handle, _thread) = MockThread::spawn(|_thread| {
        println!("hello, world!");
    });

    handle.unwrap().join().unwrap();
    println!("end!");
}
