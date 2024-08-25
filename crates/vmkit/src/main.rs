use vmkit::{define_flag, mock::MockVM, runtime::options::MMTKFlags, Runtime};

struct A;

struct B;

define_flag!(A => usize, flag, 0, "A flag");
define_flag!(B => usize, flag, 0, "B flag");

fn main() {
    vmkit::utils::flags::parse_with_prefix::<MMTKFlags>("gc", std::env::args(), std::env::vars())
        .unwrap();

    let _vmkit = MockVM::vmkit();
}
