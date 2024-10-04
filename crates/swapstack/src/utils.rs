/// Align up an integer to the given alignment. `align` must be a power of two.
pub const fn raw_align_up(val: usize, align: usize) -> usize {
    // See https://github.com/rust-lang/rust/blob/e620d0f337d0643c757bab791fc7d88d63217704/src/libcore/alloc.rs#L192
    val.wrapping_add(align).wrapping_sub(1) & !align.wrapping_sub(1)
}
