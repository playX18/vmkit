use crate::{
    scanning::{Tracer, Visitor},
    Runtime,
};
use std::num::NonZeroUsize;

/// The "metadata" for an allocated GC cell: the kind of cell, and
/// methods to "mark" (really, to invoke a GC callback on values in
/// the block) and (optionally) finalize the cell.
#[repr(C)]
pub struct VTable<R: Runtime> {
    /// This is used to ensure a VTable is valid and is not some other value.
    /// Notably, since VTable can sometimes be a forwarding pointer, if a pointer
    /// to a VTable is accidentally used as a ObjectReference, it will try to use the first
    /// word as another VTable.
    /// By putting in a magic number here, it will
    /// SIGSEGV on a specific address, which will make it easy to know exactly
    /// what went wrong.
    /// This is left on even in opt builds because VTable sizes are not
    /// particularly important.
    pub magic: u64,
    /// `size` should be the size of the cell if it is fixed, or 0 if it is
    /// variable sized.
    /// If it is variable sized, it should have `
    pub size: usize,
    pub alignment: NonZeroUsize,
    /// A callback to compute object size in case it's not static (e.g arrays or strings).
    pub compute_size: Option<extern "C" fn(*const ()) -> NonZeroUsize>,
    /// A callback to trace object fields.
    pub trace: TraceCallback<R>,
    pub finalize: FinalizeCallback,
}

pub enum TraceCallback<R: Runtime> {
    /// Object supports enqueing slots to fields (slot == reference to field).
    ///
    /// When scanning provide field pointers to MMTk:
    /// ```rust,must_fail
    /// fn trace(obj: &mut MyObject, vis: &mut dyn SlotVisitor<SimpleSlot>) {
    ///     vis.visit_slot(SimpleSlot::from_address(&obj.field));
    /// }
    ///
    /// ```
    ScanSlots(fn(*mut (), &mut Visitor<R>)),
    /// Object can only scan fields directly. In this case you're supposed to do something like:
    /// ```rust,must_fail
    /// fn trace(obj: &mut MyObject, tracer: &mut dyn ObjectTracer) {
    ///     obj.field = tracer.trace_object(obj.field);
    /// }
    ///
    /// ```
    ScanObjects(fn(*mut (), &mut Tracer<R>)),
    /// Object does not require tracing
    NoTrace,
}

pub enum FinalizeCallback {
    Finalize(fn(*mut ())),
    Drop(fn(*mut ())),
    None,
}

impl<R: Runtime> VTable<R> {
    /// Value is 64 bits to make sure it can be used as a pointer in both 32 and
    /// 64-bit builds.
    /// "57ab1e" == "vtable".
    /// ff added at the beginning to make sure it's a kernel address (even in a
    /// 32-bit build).
    pub const MAGIC: u64 = 0xff57ab1eff57ab1e;

    pub fn size(&self) -> usize {
        self.size
    }
}
