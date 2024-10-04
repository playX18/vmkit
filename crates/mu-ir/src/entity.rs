use cranelift_entity::*;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct MuEntity(u32);

entity_impl!(MuEntity);
