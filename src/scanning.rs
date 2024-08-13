use std::marker::PhantomData;

use mmtk::{util::ObjectReference, vm::Scanning};

use crate::{
    objectmodel::{header::HeapObjectHeader, reference::*, vtable::TraceCallback},
    MMTKLibAlloc, Runtime,
};

pub struct VMScanning;

impl<R: Runtime> Scanning<MMTKLibAlloc<R>> for VMScanning {
    fn support_slot_enqueuing(
        _tls: mmtk::util::VMWorkerThread,
        object: mmtk::util::ObjectReference,
    ) -> bool {
        let object = <&HeapObjectHeader<R>>::from(object);
        let vt = object.vtable();

        matches!(vt.trace, TraceCallback::ScanSlots(_))
    }

    fn scan_object<SV: mmtk::vm::SlotVisitor<<MMTKLibAlloc<R> as mmtk::vm::VMBinding>::VMSlot>>(
        _tls: mmtk::util::VMWorkerThread,
        object: mmtk::util::ObjectReference,
        slot_visitor: &mut SV,
    ) {
        let header = <&HeapObjectHeader<R>>::from(object);
        let vt = header.vtable();

        let TraceCallback::ScanSlots(scan) = vt.trace else {
            unreachable!()
        };
        let mut sv = |slot| slot_visitor.visit_slot(slot);
        let mut vis = Visitor {
            sv: &mut sv as &mut dyn FnMut(R::Slot),
        };

        scan(
            object.to_address::<MMTKLibAlloc<R>>().to_mut_ptr(),
            &mut vis,
        );
    }

    fn scan_object_and_trace_edges<OT: mmtk::vm::ObjectTracer>(
        _tls: mmtk::util::VMWorkerThread,
        object: mmtk::util::ObjectReference,
        object_tracer: &mut OT,
    ) {
        let header = <&HeapObjectHeader<R>>::from(object);
        let vt = header.vtable();

        let mut sv = |objref| object_tracer.trace_object(objref);

        let mut vis = Tracer {
            marker: PhantomData,
            sv: &mut sv,
        };

        let TraceCallback::ScanObjects(scan) = vt.trace else {
            return;
        };

        scan(
            object.to_address::<MMTKLibAlloc<R>>().to_mut_ptr(),
            &mut vis,
        );
    }

    fn notify_initial_thread_scan_complete(_partial_scan: bool, _tls: mmtk::util::VMWorkerThread) {}

    fn forward_weak_refs(
        _worker: &mut mmtk::scheduler::GCWorker<MMTKLibAlloc<R>>,
        _tracer_context: impl mmtk::vm::ObjectTracerContext<MMTKLibAlloc<R>>,
    ) {
    }

    fn prepare_for_roots_re_scanning() {}

    fn process_weak_refs(
        _worker: &mut mmtk::scheduler::GCWorker<MMTKLibAlloc<R>>,
        _tracer_context: impl mmtk::vm::ObjectTracerContext<MMTKLibAlloc<R>>,
    ) -> bool {
        false
    }

    fn scan_roots_in_mutator_thread(
        _tls: mmtk::util::VMWorkerThread,
        _mutator: &'static mut mmtk::Mutator<MMTKLibAlloc<R>>,
        _factory: impl mmtk::vm::RootsWorkFactory<<MMTKLibAlloc<R> as mmtk::vm::VMBinding>::VMSlot>,
    ) {
        todo!()
    }

    fn scan_vm_specific_roots(
        _tls: mmtk::util::VMWorkerThread,
        _factory: impl mmtk::vm::RootsWorkFactory<<MMTKLibAlloc<R> as mmtk::vm::VMBinding>::VMSlot>,
    ) {
        todo!()
    }

    fn supports_return_barrier() -> bool {
        false
    }
}

pub struct Visitor<'a, R: Runtime> {
    sv: &'a mut dyn FnMut(R::Slot),
}

impl<'a, R: Runtime> Visitor<'a, R> {
    pub fn visit_member<T, Tag: 'static>(&mut self, member: &BasicMember<T, Tag>) {
        if std::any::TypeId::of::<Tag>() == std::any::TypeId::of::<StrongMemberTag>() {
            let slot = member.slot::<R>();
            (self.sv)(slot);
        }
    }
}

pub struct Tracer<'a, R: Runtime> {
    sv: &'a mut dyn FnMut(ObjectReference) -> ObjectReference,
    marker: PhantomData<R>,
}

impl<'a, R: Runtime> Tracer<'a, R> {
    pub fn visit_member<T, Tag: 'static>(
        &mut self,
        member: BasicMember<T, Tag>,
    ) -> BasicMember<T, Tag> {
        if std::any::TypeId::of::<Tag>() == std::any::TypeId::of::<StrongMemberTag>() {
            if let Some(objref) = member.object_reference::<R>() {
                BasicMember::from_object_reference::<R>((self.sv)(objref))
            } else {
                member
            }
        } else {
            todo!()
        }
    }
}
