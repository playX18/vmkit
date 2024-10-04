use crate::{
    mm::scanning::{Tracer, Visitor},
    Runtime,
};

pub trait ScanSlots<R: Runtime> {
    fn scan(&self, visitor: &mut Visitor<R>) {
        let _ = visitor;
    }
}

pub trait TraceRefs<R: Runtime> {
    fn trace(&mut self, tracer: &mut Tracer<R>) {
        let _ = tracer;
    }
}
