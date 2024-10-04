use std::{
    alloc::Layout,
    marker::PhantomData,
    mem::MaybeUninit,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use mmtk::util::ObjectReference;

use crate::{Runtime, SlotOf};

use super::slot::SlotExt;

pub trait Rootable<R: Runtime> {
    /// Convert this rootable value to slot which holds any heap objects.
    fn to_slot(&mut self) -> SlotOf<R>;
}

/// A pool of shadow-stacks for threads to use. This type is thread-safe and
/// is accessed by multiple threads in order to acquire shadow stacks.
pub struct ShadowStackPool {}

pub type ShadowStackRef<R, T> = Arc<ShadowStack<R, T>>;

#[repr(C)]
pub struct ShadowStack<R: Runtime, T: Rootable<R>> {
    pub base: AtomicUsize,
    pub top: AtomicUsize,
    marker: PhantomData<(&'static R, *mut T)>,
}

#[repr(C)]
pub struct RootsFrame<'a, R: Runtime, T: Rootable<R> + 'a> {
    pub shadow_stack: &'a ShadowStack<R, T>,
    num_roots: usize,
    top: usize,
}

impl<'a, R: Runtime, T: Rootable<R> + 'a> RootsFrame<'a, R, T> {
    /// Save reference to `T` on shadow-stack frame. Must be valid for entire lifetime of the
    /// frame.
    pub fn save_root(&self, index: usize, value: &'a mut T) {
        assert!(index < self.num_roots, "Too many roots");
        unsafe {
            let p = self.top as *mut &'a mut T;

            p.add(index).write(value);
        }
    }

    /*pub fn restore_root(&self, index: usize, value: &mut T) {
        assert!(index < self.num_roots, "Too many roots");
        unsafe {
            let p = self.top as *mut T;
            *value = p.add(index).read();
        }
    }*/
}

impl<R: Runtime, T: Rootable<R>> ShadowStack<R, T> {
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

    pub fn enter_roots_frame<'a>(&'a self, num_roots: usize) -> RootsFrame<'a, R, T> {
        let top = self
            .top
            .fetch_add(num_roots * size_of::<T>(), Ordering::Relaxed);
        RootsFrame {
            shadow_stack: self,
            num_roots,
            top,
        }
    }

    pub fn leave_roots_frame(frame: &RootsFrame<'_, R, T>) {
        frame.shadow_stack.top.store(frame.top, Ordering::Relaxed);
        unsafe {
            let p = frame.top as *mut MaybeUninit<T>;

            for i in 0..frame.num_roots {
                p.add(i).write(MaybeUninit::zeroed());
            }
        }
    }
}

impl<'a, R: Runtime, T: Rootable<R>> Drop for RootsFrame<'a, R, T> {
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

/// Create a shadow-stack frame and execute expression `$e` inside of it.
///
/// This macro will put all variables into the shadow-stack `$shadow_stack`
/// and then restore them once the frame is expired.
///
/// Example:
/// ```rust,must_fail
/// let mut x = ...;
/// // shadow stack which holds roots of ObjectReference type.
/// let mut stack = ShadowStack::<MyRuntime, ObjectReference>::new();
/// shadow_frame!(stack => x : gc());
/// /* x is still alive here */
/// ```
#[macro_export]
macro_rules! shadow_frame {
    ($shadow_stack: expr => $($var: ident),* : $e: expr) => {
        let num_roots = count!($($var),*);

        {
            let frame = $shadow_stack.enter_roots_frame(num_roots);
            let mut ix = 0;
            $(
                frame.save_root(ix, &mut $var);
                ix += 1;
            )*
            let result = $e;

            drop(frame);

            result
        }
    };
}

impl<R: Runtime> Rootable<R> for ObjectReference {
    fn to_slot(&mut self) -> SlotOf<R> {
        <SlotOf<R> as SlotExt<R>>::from_pointer(self as *const Self as *mut ObjectReference)
    }
}
