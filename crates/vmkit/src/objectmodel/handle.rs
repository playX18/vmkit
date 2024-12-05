//! # Handles to GC objects

use std::{
    cell::Cell,
    mem::MaybeUninit,
    ptr::{null_mut, NonNull},
    sync::Arc,
};
use mmtk::{util::ObjectReference, vm::RootsWorkFactory};
use crate::{Runtime, SlotOf};

/// A trait representing value stored in a [Handle].
///
/// Note that value can be a non GC-ed object as well, this might be the case
/// when it holds null pointer or immediate value.
pub trait HandleValue<R: Runtime> {
    /// Slot of the value stored in a handle. In typical cases just `Slot::from(self)`. Can return None
    /// if value is not a GC object.
    fn slot(&self) -> Option<SlotOf<R>> {
        None
    }

    /// Object reference behind this value. Can return None if value is not a GC object.
    fn object_reference(&self) -> Option<ObjectReference> {
        None
    }
}

/// Handles are used in the VMKit to ensure that access
/// to MMTk objects in the virtual machine code is done in a
/// Garbage Collection safe manner.
///
/// The type Handles is the basic type that implements creation of handles and
// manages their life cycle.

pub struct Handles<T: HandleValue<R>, R: Runtime> {
    first_scoped_block: HandlesBlock<T, R>,
    scoped_blocks: Cell<NonNull<HandlesBlock<T, R>>>,
}

impl<T: HandleValue<R>, R: Runtime> Handles<T, R> {
    pub fn new() -> Arc<Self> {
        let mut this = Arc::new(Self {
            first_scoped_block: HandlesBlock::new(),
            scoped_blocks: Cell::new(NonNull::new(1 as *mut _).unwrap()),
        });
        let b = &mut Arc::get_mut(&mut this).unwrap().first_scoped_block as *mut HandlesBlock<T, R>;
        this.scoped_blocks.set(NonNull::new(b).unwrap());

        this
    }

    pub fn scan_roots(&self, factory: &mut impl RootsWorkFactory<SlotOf<R>>) {
        unsafe {
            let mut roots = Vec::new();
            let mut block = Some(self.scoped_blocks.get().as_ref());
            while let Some(handles_block) = block {
                handles_block.visit_slots(|slot| {
                    roots.push(slot);
                });
                block = handles_block.next_block.as_ref();
            }

            factory.create_process_roots_work(roots);
        }
    }

    pub fn allocate_handle(&self) -> NonNull<MaybeUninit<T>> {
        unsafe {
            if self.scoped_blocks.get().as_ref().is_full() {
                self.setup_next_scoped_block();
            }

            self.scoped_blocks.get().as_mut().allocate_handle()
        }
    }

    fn setup_next_scoped_block(&self) {
        unsafe {
            if self.scoped_blocks.get().as_ref().next_block.is_null() {
                let block = HandlesBlock::new();
                self.scoped_blocks.get().as_mut().next_block = Box::into_raw(Box::new(block));
            }

            self.scoped_blocks.set(
                NonNull::new(self.scoped_blocks.get().as_ref().next_block)
                    .expect("should be non null"),
            );
            self.scoped_blocks.get().as_mut().next_handle_slot = 0;
        }
    }
}

struct HandlesBlock<T: HandleValue<R>, R: Runtime> {
    next_block: *mut HandlesBlock<T, R>,
    next_handle_slot: usize,
    data: [MaybeUninit<T>; 64],
}

impl<T: HandleValue<R>, R: Runtime> HandlesBlock<T, R> {
    fn new() -> Self {
        Self {
            data: unsafe { MaybeUninit::zeroed().assume_init() },
            next_block: null_mut(),
            next_handle_slot: 0,
        }
    }
    fn is_full(&self) -> bool {
        self.next_handle_slot >= 64
    }

    fn allocate_handle(&mut self) -> NonNull<MaybeUninit<T>> {
        debug_assert!(!self.is_full());
        let addr = NonNull::from(&self.data[self.next_handle_slot]);
        self.next_handle_slot += 1;
        addr
    }

    fn visit_slots(&self, mut visitor: impl FnMut(SlotOf<R>)) {
        for i in 0..self.next_handle_slot {
            unsafe {
                let slot = self.data[i].assume_init_ref().slot();
                if let Some(slot) = slot {
                    visitor(slot)
                }
            }
        }
    }
}


/// HandleScope is used to start a new handles scope in the code.
/// It is used as follows:
/// 
/// 
/// ```must_fail
/// let scope = HandleScope::new(&my_handles);
/// 
/// let value: &mut Value = scope.handle(value); // `value` is valid for lifetime of a `scope`.
/// ```
pub struct HandleScope<T: HandleValue<R>, R: Runtime> {
    saved_handle_block: NonNull<HandlesBlock<T, R>>,
    saved_handle_slot: usize,
    handles: Arc<Handles<T, R>>,
}

impl<T: HandleValue<R>, R: Runtime> HandleScope<T, R> {
    pub fn new(handles: &Arc<Handles<T, R>>) -> Self {
        Self {
            saved_handle_block: handles.scoped_blocks.get(),
            saved_handle_slot: unsafe { handles.scoped_blocks.get().as_ref().next_handle_slot },
            handles: handles.clone(),
        }
    }

    pub fn handle<'a>(&self, value: T) -> &'a mut T {
        unsafe {
            let mut ptr = self.handles.allocate_handle();
            ptr.write(MaybeUninit::new(value));

            ptr.as_mut().assume_init_mut()
        }
    }
}

impl<T: HandleValue<R>, R: Runtime> Drop for HandleScope<T, R> {
    fn drop(&mut self) {
        unsafe {
            self.handles.scoped_blocks.set(self.saved_handle_block);
            self.handles.scoped_blocks.get().as_mut().next_handle_slot = self.saved_handle_slot;
        }
    }
}
