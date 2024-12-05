use vmkit::{mock::*, objectmodel::traits::ScanSlots};
use vmkit_derive::Managed;
use vmkit::objectmodel::traits::VTableDef;

#[derive(Managed)]
#[managed(runtime=MockVM, trace_mode=scan_slots)]
#[arraylike(len=len, data=data, data_type=T)]
pub struct Array<T: ScanSlots<MockVM>> {
    pub len: usize,
    #[skip]
    pub data: [T; 0]
} 

fn main() {
    println!("{:p}", &Array::<i32>::VTABLE);
}