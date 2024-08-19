/*use std::marker::PhantomData;

use mmtk::util::ObjectReference;

use crate::Runtime;

use super::traits::TraceRefs;

pub struct Ephemeron<R: Runtime> {
    pub(crate) key: Option<ObjectReference>,
    pub(crate) value: Option<ObjectReference>,
    marker: PhantomData<R>,
}

impl<R: Runtime> Ephemeron<R> {
    pub fn key(&self) -> Option<ObjectReference> {
        self.key
    }

    pub fn value(&self) -> Option<ObjectReference> {
        self.value
    }
}

impl<R: Runtime> TraceRefs<R> for Ephemeron<R> {
    fn trace(&mut self, tracer: &mut crate::mm::scanning::Tracer<R>) {
        tracer.register_weak_callback(
            self,
            Box::new(|addr, tracer| {
                let ephemeron = unsafe { addr.cast::<Self>().as_mut().unwrap() };

                if let Some(key) = ephemeron.key.filter(|key| key.is_reachable()) {
                    ephemeron.key = Some(tracer.trace_object_reference(key));
                    ephemeron.value = Some(
                        tracer.trace_object_reference(ephemeron.value.expect("cannot be none")),
                    );
                } else {
                    ephemeron.key = None;
                    ephemeron.value = None;
                }
            }),
        );
    }
}
*/
