/*use std::{
    alloc::Layout,
    marker::PhantomData,
    mem::MaybeUninit,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use mmtk::util::ObjectReference;

pub trait Rootable {
    /// An object reference stored in rootable type.
    fn get_object_reference(&self) -> Option<ObjectReference>;
}

/// A pool of shadow-stacks for threads to use. This type is thread-safe and
/// is accessed by multiple threads in order to acquire shadow stacks.
pub struct ShadowStackPool {}

pub type ShadowStackRef<T> = Arc<ShadowStack<T>>;

#[repr(C)]
pub struct ShadowStack<T: Rootable> {
    pub base: AtomicUsize,
    pub top: AtomicUsize,
    marker: PhantomData<*const T>,
}

#[repr(C)]
pub struct RootsFrame<'a, T: Rootable + 'a> {
    pub shadow_stack: &'a ShadowStack<T>,
    num_roots: usize,
    top: usize,
}

impl<'a, T: Rootable + 'a> RootsFrame<'a, T> {
    pub fn save_root(&self, index: usize, value: T) {
        assert!(index < self.num_roots, "Too many roots");
        unsafe {
            let p = self.top as *mut T;

            p.add(index).write(value);
        }
    }

    pub fn restore_root(&self, index: usize, value: &mut T) {
        assert!(index < self.num_roots, "Too many roots");
        unsafe {
            let p = self.top as *mut T;
            *value = p.add(index).read();
        }
    }
}

impl<T: Rootable> ShadowStack<T> {
    pub fn new() -> Self {
        unsafe {
            let mem = std::alloc::alloc_zeroed(Layout::array::<T>(16 * 1024).unwrap());

            Self {
                base: AtomicUsize::new(mem as _),
                top: AtomicUsize::new(mem as _),
                marker: PhantomData,
            }
        }
    }

    pub fn enter_roots_frame<'a>(&'a self, num_roots: usize) -> RootsFrame<'a, T> {
        let top = self
            .top
            .fetch_add(num_roots * size_of::<T>(), Ordering::Relaxed);
        RootsFrame {
            shadow_stack: self,
            num_roots,
            top,
        }
    }

    pub fn leave_roots_frame(frame: &RootsFrame<'_, T>) {
        frame.shadow_stack.top.store(frame.top, Ordering::Relaxed);
        unsafe {
            let p = frame.top as *mut MaybeUninit<T>;

            for i in 0..frame.num_roots {
                p.add(i).write(MaybeUninit::zeroed());
            }
        }
    }
}

impl<'a, T: Rootable> Drop for RootsFrame<'a, T> {
    fn drop(&mut self) {
        ShadowStack::leave_roots_frame(self);
    }
}

#[macro_export]
macro_rules! count {
    ($var: ident, $($rest:tt)*) => {
        count!(@count 1; $($rest)*)
    };

    (@count $c: expr;) => {
        $c
    };

    (@count $c: expr; $var:ident, $($rest:tt)*) => {
        count!(@count $c + 1; $($rest)*);
    };

    () => {
        0
    }
}

#[macro_export]
macro_rules! shadow_frame {
    ($shadow_stack: expr => $($var: ident),* : $e: expr) => {
        let num_roots = count!($($var),*);

        {
            let frame = $shadow_stack.enter_roots_frame(num_roots);
            let mut ix = 0;
            $(
                frame.save_root(ix, $var);
                ix += 1;
            )*
            let result = $e;


            let mut ix = 0;
            $(
                frame.restore_root(ix, &mut $var);
                ix += 1;
            )*
            drop(frame);

            result
        }
    };
}

impl Rootable for ObjectReference {
    fn get_object_reference(&self) -> Option<ObjectReference> {
        Some(*self)
    }
}

*/
