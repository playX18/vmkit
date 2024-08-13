pub const OBJECT_HEADER_OFFSET: isize = -(size_of::<usize>() as isize);
pub const OBJECT_HASH_SIZE: usize = size_of::<u64>();
pub const OBJECT_HASH_OFFSET: isize = OBJECT_HEADER_OFFSET + -(OBJECT_HASH_SIZE as isize);
pub const OBJECT_REF_OFFSET: usize = (-OBJECT_HEADER_OFFSET) as usize;
