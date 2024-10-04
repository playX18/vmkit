use std::marker::PhantomData;

use mmtk::util::ObjectReference;

use crate::{runtime::threads::*, Runtime, ThreadOf};

pub struct LockStack<R: Runtime> {
    top: u32,
    base: [Option<ObjectReference>; 8],
    marker: PhantomData<&'static R>,
}

impl<R: Runtime> LockStack<R> {
    pub const fn new() -> Self {
        Self {
            top: 0,
            base: [None; 8],
            marker: PhantomData,
        }
    }

    pub fn is_owning_thread(&self) -> bool {
        let current = vmkit_current_thread();

        if ThreadOf::<R>::is_mutator(current) {
            let tls = ThreadOf::<R>::tls(current);
            return std::ptr::eq(&tls.lock_stack, self);
        }

        false
    }

    pub fn push(&mut self, obj: ObjectReference) {
        assert!(!self.contains(obj));
        assert!(!self.is_full());
        self.base[self.top as usize] = Some(obj);
        self.top += 1;
    }

    pub fn bottom(&self) -> ObjectReference {
        self.base[0].expect("must contain an object")
    }

    pub fn is_empty(&self) -> bool {
        self.top == 0
    }

    pub fn is_recursive(&self, o: ObjectReference) -> bool {
        // This will succeed iff there is a consecutive run of oops on the
        // lock-stack with a length of at least 2.
        let end = self.top as usize;

        // Start iterating from the top because the runtime code is more
        // interested in the balanced locking case when the top oop on the
        // lock-stack matches o. This will cause the for loop to break out
        // in the first loop iteration if it is non-recursive.
        for i in (0..end).rev() {
            if self.base[i - 1] == Some(o) && self.base[i] == Some(o) {
                return true;
            }
            // o can only occur in one consecutive run on the lock-stack.
            // Only one of the two oops checked matched o, so this run
            // must be of length 1 and thus not be recursive. Stop the search.
            if self.base[i] == Some(o) {
                break;
            }
        }

        false
    }

    #[inline]
    pub fn try_recursive_enter(&mut self, o: ObjectReference) -> bool {
        let end = self.top as usize;
        if end == 0 || self.base[end - 1] != Some(o) {
            // topmost obj does not match o
            return false;
        }

        self.base[end] = Some(o);
        self.top += 1;
        true
    }

    #[inline]
    pub fn try_recursive_exit(&mut self, o: ObjectReference) -> bool {
        let end = self.top as usize;
        if end <= 1 || self.base[end - 1] != Some(o) || self.base[end - 2] != Some(o) {
            return false;
        }

        self.top -= 1;
        true
    }

    #[inline]
    pub fn remove(&mut self, o: ObjectReference) -> usize {
        let end = self.top as usize;
        let mut inserted = 0;

        for i in 0..end {
            if self.base[i] != Some(o) {
                if inserted != i {
                    self.base[inserted] = self.base[i];
                }
                inserted += 1;
            }
        }

        let removed = end - inserted;
        self.top -= removed as u32;

        removed
    }

    pub fn contains(&self, obj: ObjectReference) -> bool {
        self.base.iter().any(|x| x == &Some(obj))
    }

    pub fn is_full(&self) -> bool {
        self.top == 8
    }
}
