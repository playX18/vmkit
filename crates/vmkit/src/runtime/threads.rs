use crate::{
    mm::tlab::TLAB,
    runtime::thunks::thread_start,
    sync::{Monitor, MonitorGuard},
    MMTKVMKit, Runtime, ThreadOf,
};
use mmtk::{
    util::{Address, OpaquePointer, VMMutatorThread, VMThread},
    vm::RootsWorkFactory,
    Mutator,
};
use stack::Stack;
use std::{
    cell::{Cell, RefCell, UnsafeCell},
    marker::PhantomData,
    mem::{transmute, MaybeUninit},
    ptr::{null_mut, NonNull},
    sync::{
        atomic::{AtomicBool, AtomicI32, AtomicI8, AtomicU8, AtomicUsize, Ordering},
        Condvar, Mutex,
    },
    thread::JoinHandle,
};

pub trait Thread<R: Runtime>: 'static {
    /// A list of block adapters that can be used to block a thread.
    type BlockAdapterList: BlockAdapterList<R>;

    fn new(mutator: bool, tls: TLSData<R>) -> VMThread;
    fn set_index_in_thread_list(thread: VMThread, ix: usize) {
        let tls = ThreadOf::<R>::tls(thread);
        tls.index_in_thread_list.store(ix, Ordering::Relaxed);
    }
    fn index_in_thread_list(thread: VMThread) -> usize {
        ThreadOf::<R>::tls(thread)
            .index_in_thread_list
            .load(Ordering::Relaxed)
    }
    /// Unique thread ID. This is used for implementation of GC-safe sync primitives.
    fn id(thread: VMThread) -> u64;
    fn tls<'a>(thread: VMThread) -> &'a TLSData<R>;
    fn is_mutator(_thread: VMThread) -> bool {
        true
    }

    unsafe fn swapstack(stackref: *mut Stack, arg: usize) -> usize {
        let func: unsafe extern "C" fn(*mut Stack, usize) -> usize =
            transmute(R::vmkit().thunks.swapstack.start());

        func(stackref, arg)
    }

    /// Start a thread.
    ///
    /// # Safety
    ///
    /// `stack` must be a valid pointer and live for the entire thread runtime.
    unsafe fn start(
        thread: VMThread,
        stack: NonNull<Stack>,
        arg: usize,
    ) -> std::io::Result<JoinHandle<()>> {
        let tls = ThreadOf::<R>::tls(thread);

        tls.stack.set(stack.as_ptr());

        std::thread::Builder::new().spawn(move || {
            let tls = ThreadOf::<R>::tls(thread);

            let mutator = if ThreadOf::<R>::is_mutator(thread) {
                let mutator =
                    mmtk::memory_manager::bind_mutator(&R::vmkit().mmtk, VMMutatorThread(thread));
                (tls.mutator.as_ptr() as *mut Box<Mutator<_>>).write(mutator);
                true
            } else {
                false
            };

            if main_thread() == VMThread::UNINITIALIZED {
                MAIN_THREAD.store(thread.0.to_address().as_usize(), Ordering::Relaxed);
                R::vmkit().threads.add_main_thread(thread);
            } else {
                R::vmkit().threads.add_thread(thread);
            }
            THREAD.with_borrow_mut(|thr| *thr = thread);
            tls.set_state(ThreadState::Running);

            let stack = tls.stack.get();
            let mut native = Stack::uninit();
            let pinned = std::pin::Pin::new(&mut native);
            let ptr = pinned.get_mut() as *mut Stack;
            tls.native_sp.set(ptr);

            thread_start::<R>(stack, arg);

            let _ = pinned;
            if mutator {
                let mutator = tls.mutator.as_ptr() as *mut Box<Mutator<MMTKVMKit<R>>>;

                mmtk::memory_manager::destroy_mutator(&mut **mutator);

                let _ = mutator.read();
            }
        })
    }
    fn save_thread_state();

    fn scan_roots(thread: VMMutatorThread, factory: impl RootsWorkFactory<R::Slot>);

    fn acknowledge_block_requests<'a>(thread: VMThread) -> Option<MonitorGuard<'a, (), R, false>> {
        if Self::BlockAdapterList::acknowledge_block_requests(thread) {
            Some(Self::tls(thread).monitor.lock_no_handshake())
        } else {
            None
        }
    }

    fn is_blocked(thread: VMThread) -> bool {
        Self::BlockAdapterList::is_blocked(thread)
    }

    ///
    /// Check if the thread is supposed to block, and if so, block it. This method
    /// will ensure that soft handshake requests are acknowledged or else
    /// inhibited, that any blocking request is handled, that the execution state
    /// of the thread is set to <code>Running</code>
    /// once all blocking requests are cleared, and that other threads are notified
    /// that this thread is in the middle of blocking by setting the appropriate
    /// flag (<code>is_blocking</code>). Note that this thread acquires the
    /// monitor(), though it may release it completely either by calling wait() or
    /// by calling unlock_completely(). Thus, although it isn't generally a problem
    /// to call this method while holding the monitor() lock, you should only do so
    /// if the loss of atomicity is acceptable.
    /// <p>
    /// Generally, this method should be called from the following four places:
    /// <ol>
    /// <li>The block() method, if the thread is requesting to block itself.
    /// Currently such requests only come when a thread calls suspend(). Doing so
    /// has unclear semantics (other threads may call resume() too early causing
    /// the well-known race) but must be supported because it's still part of the
    /// JDK. Why it's safe: the block() method needs to hold the monitor() for the
    /// time it takes it to make the block request, but does not need to continue
    /// to hold it when it calls checkBlock(). Thus, the fact that checkBlock()
    /// breaks atomicity is not a concern.
    /// <li>The yieldpoint. One of the purposes of a yieldpoint is to periodically
    /// check if the current thread should be blocked. This is accomplished by
    /// calling checkBlock(). Why it's safe: the yieldpoint performs several
    /// distinct actions, all of which individually require the monitor() lock -
    /// but the monitor() lock does not have to be held contiguously. Thus, the
    /// loss of atomicity from calling checkBlock() is fine.
    /// <li>The "WithHandshake" methods of HeavyCondLock. These methods allow you to
    /// block on a mutex or condition variable while notifying the system that you
    /// are not executing Java code. When these blocking methods return, they check
    /// if there had been a request to block, and if so, they call checkBlock().
    /// Why it's safe: This is subtle. Two cases exist. The first case is when a
    /// WithHandshake method is called on a HeavyCondLock instance that is not a thread
    /// monitor(). In this case, it does not matter that checkBlock() may acquire
    /// and then completely release the monitor(), since the user was not holding
    /// the monitor(). However, this will break if the user is <i>also</i> holding
    /// the monitor() when calling the WithHandshake method on a different lock. This case
    /// should never happen because no other locks should ever be acquired when the
    /// monitor() is held. Additionally: there is the concern that some other locks
    /// should never be held while attempting to acquire the monitor(); the
    /// HeavyCondLock ensures that checkBlock() is only called when that lock
    /// itself is released. The other case is when a WithHandshake method is called on the
    /// monitor() itself. This should only be done when using <i>your own</i>
    /// monitor() - that is the monitor() of the thread your are running on. In
    /// this case, the WithHandshake methods work because: (i) lockWithHandshake() only calls
    /// checkBlock() on the initial lock entry (not on recursive entry), so
    /// atomicity is not broken, and (ii) waitWithHandshake() and friends only call
    /// checkBlock() after wait() returns - at which point it is safe to release
    /// and reacquire the lock, since there cannot be a race with broadcast() once
    /// we have committed to not calling wait() again.
    /// <li>Any code following a potentially-blocking native call. Case (3) above
    /// is somewhat subsumed in this except that it is special due to the fact that
    /// it's blocking on VM locks. So, this case refers specifically to JNI. The
    /// JNI epilogues will call leaveJNIBlocked(), which calls a variant of this
    /// method.
    /// </ol>
    ////
    fn check_block(thread: VMThread) {
        if Self::is_mutator(thread) {
            Self::save_thread_state();
        }
        Self::check_block_no_save_context(thread);
    }

    fn check_block_no_save_context(thread: VMThread) {
        let tls = Self::tls(thread);

        let mut guard = tls.monitor.lock_no_handshake();
        tls.is_blocking.store(true, Ordering::Relaxed);

        loop {
            // deal with block requests
            Self::acknowledge_block_requests(thread);
            // are we blocked?
            if !Self::is_blocked(thread) {
                break;
            }
            // what if a GC request comes while we're here for a suspend()
            // request?
            // answer: we get awoken, reloop, and acknowledge the GC block
            // request.
            guard.wait_no_handshake();
        }
        // we're about to unblock, so indicate to the world that we're running
        // again.
        tls.state
            .store(ThreadState::Running as u8, Ordering::Relaxed);
        // let everyone know that we're back to executing code\
        tls.is_blocking.store(false, Ordering::Relaxed);
        // deal with requests that came up while we were blocked.
        drop(guard);
    }

    fn unblock<B: BlockAdapter<R>>(thread: VMThread) {
        let tls = Self::tls(thread);

        let guard = tls.monitor.lock_no_handshake();
        B::clear_block_request(thread);
        B::set_blocked(thread, false);
        guard.monitor.notify_all();
        drop(guard);
    }

    fn block<B: BlockAdapter<R>>(thread: VMThread, asynchronous: bool) -> ThreadState {
        let mut result;
        let current = R::current_thread();
        let tls = Self::tls(thread);
        let mut guard = tls.monitor.lock_no_handshake();
        let token = B::request_block(thread);
        if current == thread {
            Self::check_block(thread);
            result = tls.state();
        } else {
            if tls.is_about_to_terminate.load(Ordering::Relaxed) {
                result = ThreadState::Terminated
            } else {
                tls.take_yieldpoint.store(1, Ordering::Relaxed);
                let new_state = tls.set_blocked_exec_status();
                result = new_state;

                tls.monitor.notify_all();

                if new_state == ThreadState::RunningToBlock {
                    if !asynchronous {
                        while B::has_block_request_with_token(thread, token)
                            && !B::is_blocked(thread)
                            && !tls.is_about_to_terminate.load(Ordering::Relaxed)
                        {
                            guard.wait_no_handshake();
                        }

                        if tls.is_about_to_terminate.load(Ordering::Relaxed) {
                            result = ThreadState::Terminated;
                        } else {
                            result = tls.state();
                        }
                    }
                } else if new_state == ThreadState::BlockedInParked {
                    // we own the thread for now - it cannot go back to executing managed
                    // code until we release the lock. before we do so we change its
                    // state accordingly and tell anyone who is waiting.
                    B::clear_block_request(thread);
                    B::set_blocked(thread, true);
                }
            }
        }

        drop(guard);
        result
    }

    fn blocked_for<B: BlockAdapter<R>>(thread: VMThread) -> bool {
        let guard = Self::tls(thread).monitor.lock_no_handshake();
        let res = B::is_blocked(thread);
        drop(guard);

        res
    }

    fn block_async<B: BlockAdapter<R>>(thread: VMThread) -> ThreadState {
        Self::block::<B>(thread, true)
    }

    fn block_sync<B: BlockAdapter<R>>(thread: VMThread) -> ThreadState {
        Self::block::<B>(thread, false)
    }

    /// Indicate that we'd like the current thread to be executing privileged code that
    /// does not require synchronization with the GC.  This call may be made on a thread
    /// that is [`Running`](ThreadState::Running) or [`RunningToBlock`](ThreadState::RunningToBlock), and will result in the thread being either
    /// [`Parked`](ThreadState::Parked) or [`BlockedInParked`](ThreadState::BlockedInParked).  In the case of an
    /// [`RunningToBlock`](ThreadState::RunningToBlock) -> [`BlockedInParked`](ThreadState::BlockedInParked) transition, this call will acquire the
    /// thread's lock and send out a notification to any threads waiting for this thread
    /// to reach a safepoint.  This notification serves to notify them that the thread
    /// is in GC-safe code, but will not reach an actual safepoint for an indetermined
    /// amount of time.  This is significant, because safepoints may perform additional
    /// actions (such as handling handshake requests, which may include things like
    /// mutator flushes and running isync) that [`Parked`](ThreadState::Parked) code will not perform until
    /// returning to [`Running`](ThreadState::Running) by way of a [`leave_native()`](Self::leave_parked) call.

    #[inline(never)]
    fn enter_parked() {
        let t = R::current_thread();
        let tls = Self::tls(t);
        let mut old_state;
        let mut new_state;

        loop {
            old_state = tls.state();
            if old_state == ThreadState::Running {
                new_state = ThreadState::Parked;
            } else {
                Self::enter_parked_blocked(VMMutatorThread(t));
                return;
            }

            if tls.attempt_fast_exec_status_transition(old_state, new_state) {
                break;
            }
        }
    }

    /// Internal method for transitioning a thread from [`Running`](ThreadState::Running) or [`RunningToBlock`](ThreadState::RunningToBlock) to
    /// [`BlockedInParked`](ThreadState::BlockedInParked). It is always safe to conservatively call this method when transitioning
    /// to native code, though it is faster to call [`enter_parked()`](Self::enter_parked).
    /// This method takes care of all bookkeeping and notifications required when a
    /// a thread that has been requested to block instead decides to run native code.
    /// Threads enter native code never need to block, since they will not be executing
    /// any managed code.  However, such threads must ensure that any system services (like
    /// GC) that are waiting for this thread to stop are notified that the thread has
    /// instead chosen to exit managed code.  As well, any requests to perform a soft handshake
    /// must be serviced and acknowledged.
    fn enter_parked_blocked(thread: VMMutatorThread) {
        let tls = Self::tls(thread.0);
        let guard = tls.monitor.lock_no_handshake();
        tls.set_exec_status(ThreadState::BlockedInParked);
        Self::acknowledge_block_requests(thread.0);
        drop(guard);
    }
    fn attempt_leave_parked_no_block() -> bool {
        let t = R::current_thread();
        let tls = Self::tls(t);
        loop {
            let old_state = tls.state();

            let new_state = if old_state == ThreadState::Parked {
                ThreadState::Running
            } else {
                return false;
            };

            if tls.attempt_fast_exec_status_transition(old_state, new_state) {
                break true;
            }
        }
    }

    fn leave_parked() {
        Self::check_block_no_save_context(R::current_thread());
    }

    fn yieldpoints_enabled(thread: VMMutatorThread) -> bool {
        Self::tls(thread.0)
            .yieldpoints_enabled_count
            .load(Ordering::Relaxed)
            == 1
    }

    fn enable_yieldpoints(thread: VMMutatorThread) {
        let tls = Self::tls(thread.0);
        tls.yieldpoints_enabled_count
            .fetch_add(1, Ordering::Relaxed);

        if Self::yieldpoints_enabled(thread)
            && tls.yieldpoint_request_pending.load(Ordering::Relaxed)
        {
            tls.take_yieldpoint.store(1, Ordering::Relaxed);
            tls.yieldpoint_request_pending
                .store(false, Ordering::Relaxed);
        }
    }

    fn disable_yieldpoints(thread: VMMutatorThread) {
        Self::tls(thread.0)
            .yieldpoints_enabled_count
            .fetch_sub(1, Ordering::Relaxed);
    }

    /// Check if thread should take a [`yieldpoint`](Thread::yieldpoint).
    ///
    /// Params are passed to yieldpoint function, read its documentation for reference.
    #[inline(always)]
    fn check_yieldpoint(where_from: i32, yieldpoint_fp: Address) {
        if Self::tls(R::current_thread())
            .take_yieldpoint
            .load(Ordering::Relaxed)
            != 0
        {
            Self::yieldpoint(where_from, yieldpoint_fp)
        }
    }

    /// Process a taken yieldpoint.
    ///
    /// Params:
    ///
    /// - `where_from`: source of yieldpoint (e.g loop backedge), it's up to runtime to pass this value and interpret it.
    /// - `yieldpoint_fp`: Potentially frame-pointer of function that invoked this yieldpoint, can be used to get a stacktrace,
    /// perform OSR, etc. It's up to runtime to pass this value and interpret it.
    fn yieldpoint(where_from: i32, yieldpoint_fp: Address) {
        let t = R::current_thread();
        // only mutators can enter yieldpoint
        if !Self::is_mutator(t) {
            return;
        }
        let tls = Self::tls(t);
        tls.at_yieldpoint.store(true, Ordering::Relaxed);
        tls.yieldpoints_taken.fetch_add(1, Ordering::Relaxed);
        // If thread is in critical section we can't do anything right now, defer
        // until later
        // we do this without acquiring locks, since part of the point of disabling
        // yieldpoints is to ensure that locks are not "magically" acquired
        // through unexpected yieldpoints. As well, this makes code running with
        // yieldpoints disabled more predictable. Note furthermore that the only
        // race here is setting take_yieldpoint to 0. But this is perfectly safe,
        // since we are guaranteeing that a yieldpoint will run after we emerge from
        // the no-yieldpoints code. At worst, setting takeYieldpoint to 0 will be
        // lost (because some other thread sets it to non-0), but in that case we'll
        // just come back here and reset it to 0 again.
        if !Self::yieldpoints_enabled(VMMutatorThread(t)) {
            tls.yieldpoint_request_pending
                .store(true, Ordering::Relaxed);
            tls.take_yieldpoint.store(0, Ordering::Relaxed);
            tls.at_yieldpoint.store(false, Ordering::Relaxed);
            return;
        }

        tls.yieldpoints_taken_fully.fetch_add(1, Ordering::Relaxed);

        let guard = tls.monitor.lock_no_handshake();

        let take_yieldpoint_val = tls.take_yieldpoint.load(Ordering::Relaxed);

        if take_yieldpoint_val != 0 {
            tls.take_yieldpoint.store(0, Ordering::Relaxed);
            // do two things: check if we should be blocking, and act upon
            // handshake requests.
            Self::check_block(t);

            // perform action once yieldpoint unblocked, runtime can run finalizers,
            // OSR, etc.
            Self::yieldpoint_unblocked(VMMutatorThread(t), where_from, yieldpoint_fp);
        }

        drop(guard);
        tls.at_yieldpoint.store(false, Ordering::Relaxed);
    }

    /// An action to be performed once yieldpoint was finished. This can be anything: checking timer interrupts,
    /// performing OSR requests, etc.
    ///
    /// Thread monitor is *locked* when we're inside this function, it's safe to mutate thread here.
    fn yieldpoint_unblocked(thread: VMMutatorThread, where_from: i32, yieldpoint_fp: Address) {
        let _ = where_from;
        let _ = yieldpoint_fp;
        let _ = thread;
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum ThreadState {
    New = 0,
    /// Thread is running "normal" managed code that may contain GC pointers
    Running = 1,
    /// A state that is used to mark that a thread is in privileged code
    /// that does not synchronize with the collector.
    Parked = 2,
    /// Thread is running managed code but is expected to block. The transition from Running to
    /// RunningToBlock happens as a result of asynchronous call by the GC or any other internal
    /// VM code that requires this thread to perform an asynchronous activity.
    RunningToBlock = 3,
    /// Thread is in native code, and is to block before returning to managed code.
    BlockedInParked = 4,
    Terminated = 5,
}

impl From<u8> for ThreadState {
    fn from(value: u8) -> ThreadState {
        match value {
            0 => ThreadState::New,
            1 => ThreadState::Running,
            2 => ThreadState::Parked,
            3 => ThreadState::RunningToBlock,
            4 => ThreadState::BlockedInParked,
            5 => ThreadState::Terminated,
            _ => unreachable!(),
        }
    }
}

impl ThreadState {
    pub fn is_running(&self) -> bool {
        match *self {
            ThreadState::Running => true,
            _ => false,
        }
    }

    pub fn is_parked(&self) -> bool {
        match *self {
            ThreadState::Parked | ThreadState::BlockedInParked => true,
            _ => false,
        }
    }

    pub fn not_running(&self) -> bool {
        matches!(self, Self::New | Self::Terminated)
    }

    pub fn to_usize(&self) -> usize {
        *self as usize
    }
}

impl Default for ThreadState {
    fn default() -> ThreadState {
        ThreadState::Running
    }
}

/// Thread-local state data for MMTk. This struct stores all the data necessary
/// to allocate objects, perform write barriers, and stop the world.
#[repr(C)]
pub struct TLSData<R: Runtime> {
    /// A thread local allocation buffer. Used to allocate small enough objects *fast*.
    pub tlab: UnsafeCell<TLAB<R>>,
    /// Is currently enalbed GC generational? Available to all threads for fast checks in fast-paths.
    pub is_generational: bool,
    /// A value indicating that yieldpoint should be taken. Our crate sets it to `1` when GC is requesting
    /// yieldpoints but runtime implementing `Thread` trait can also have more meanings for this value e.g `-1`
    /// means take yieldpoint at loop backedge to start JIT compilation.
    pub take_yieldpoint: AtomicI8,
    /// A statistic counter that contains the number of fully taken yieldpoints that is when we acquire the thread
    /// lock and check for blocking requests.
    pub yieldpoints_taken_fully: AtomicUsize,
    /// A statistic counter that contains the number of taken yieldpoints that is every single time when `yieldpoint` method
    /// was invoked.
    pub yieldpoints_taken: AtomicUsize,
    /// Is yieldpoint request pending on this thread? It's only set by `enable_yieldpoints` and `disable_yieldpoints`.
    pub yieldpoint_request_pending: AtomicBool,
    pub at_yieldpoint: AtomicBool,
    /// Should this thread yield at yieldpoints? A value of: 1 means "yes"
    /// (yieldpoints enabled) &lt;= 0 means "no" (yieldpoints disabled)
    pub yieldpoints_enabled_count: AtomicI32,
    pub state: AtomicU8,
    pub is_blocking: AtomicBool,
    pub is_blocked_for_gc: AtomicBool,
    pub should_block_for_gc: AtomicBool,
    pub monitor: Monitor<(), R, false>,
    pub mutator: MaybeUninit<UnsafeCell<Box<Mutator<MMTKVMKit<R>>>>>,
    pub is_about_to_terminate: AtomicBool,
    pub stack: Cell<*mut Stack>,
    pub native_sp: Cell<*mut Stack>,
    pub index_in_thread_list: AtomicUsize,
    routine: UnsafeCell<MaybeUninit<Box<dyn FnOnce(VMThread)>>>,
    mutator_routine: UnsafeCell<MaybeUninit<Box<dyn FnOnce(VMMutatorThread)>>>,
}

impl<R: Runtime> TLSData<R> {
    pub fn new() -> Self {
        Self {
            tlab: UnsafeCell::new(TLAB::<R>::new()),
            take_yieldpoint: AtomicI8::new(0),
            yieldpoint_request_pending: AtomicBool::new(false),
            stack: Cell::new(null_mut()),
            index_in_thread_list: AtomicUsize::new(0),
            yieldpoints_enabled_count: AtomicI32::new(0),
            at_yieldpoint: AtomicBool::new(false),
            yieldpoints_taken_fully: AtomicUsize::new(0),
            yieldpoints_taken: AtomicUsize::new(0),
            is_about_to_terminate: AtomicBool::new(false),
            is_generational: R::vmkit().mmtk.get_plan().generational().is_some(),
            is_blocking: AtomicBool::new(false),
            monitor: Monitor::new(()),
            should_block_for_gc: AtomicBool::new(false),
            is_blocked_for_gc: AtomicBool::new(false),
            mutator: MaybeUninit::uninit(),
            state: AtomicU8::new(ThreadState::Running as _),
            mutator_routine: UnsafeCell::new(MaybeUninit::uninit()),
            routine: UnsafeCell::new(MaybeUninit::uninit()),
            native_sp: Cell::new(null_mut()),
        }
    }

    pub unsafe fn tlab_mut_unchecked(&self) -> &mut TLAB<R> {
        &mut *self.tlab.get()
    }

    pub unsafe fn mutator_mut_unchecked(&self) -> &mut Mutator<MMTKVMKit<R>> {
        &mut *self.mutator.assume_init_ref().get()
    }

    pub fn is_running(&self) -> bool {
        self.state().is_running()
    }

    pub fn is_parked(&self) -> bool {
        self.state().is_parked()
    }

    pub fn state(&self) -> ThreadState {
        ThreadState::from(self.state.load(Ordering::Relaxed))
    }

    pub fn set_state(&self, state: ThreadState) {
        self.state.store(state as _, Ordering::Relaxed);
    }

    pub fn attempt_fast_exec_status_transition(
        &self,
        old_state: ThreadState,
        new_state: ThreadState,
    ) -> bool {
        self.state
            .compare_exchange_weak(
                old_state as _,
                new_state as _,
                Ordering::AcqRel,
                Ordering::Relaxed,
            )
            .is_ok()
    }

    pub fn set_exec_status(&self, state: ThreadState) {
        self.state.store(state as _, Ordering::Relaxed);
    }

    pub fn set_blocked_exec_status(&self) -> ThreadState {
        let mut old_state;
        let mut new_state;

        loop {
            old_state = self.state();

            if old_state == ThreadState::Running {
                new_state = ThreadState::RunningToBlock;
            } else if old_state == ThreadState::Parked {
                new_state = ThreadState::BlockedInParked;
            } else {
                new_state = old_state;
            }

            if self.attempt_fast_exec_status_transition(old_state, new_state) {
                break new_state;
            }
        }
    }

    pub fn native_stack(&self) -> *mut Stack {
        self.native_sp.get()
    }

    pub fn stack(&self) -> *mut Stack {
        self.stack.get()
    }
}

struct BarrierData {
    armed: bool,
    stopped: usize,
}

impl BarrierData {
    pub const fn new() -> BarrierData {
        BarrierData {
            armed: false,
            stopped: 0,
        }
    }

    pub fn is_armed(&self) -> bool {
        self.armed
    }

    pub fn arm(&mut self) {
        self.stopped = 0;
        self.armed = true;
    }

    pub fn disarm(&mut self) {
        self.armed = false;
    }
}
pub struct Barrier {
    data: Mutex<BarrierData>,
    cv_wakeup: Condvar,
    cv_notify: Condvar,
}

impl Barrier {
    pub const fn new() -> Barrier {
        Barrier {
            data: Mutex::new(BarrierData::new()),
            cv_wakeup: Condvar::new(),
            cv_notify: Condvar::new(),
        }
    }

    pub fn arm(&self) {
        let mut data = self.data.lock().unwrap();
        assert!(!data.is_armed());
        data.arm();
    }

    pub fn disarm(&self) {
        let mut data = self.data.lock().unwrap();
        assert!(data.is_armed());
        data.disarm();
        self.cv_wakeup.notify_all();
    }

    pub fn notify_park(&self) {
        let mut data = self.data.lock().unwrap();
        assert!(data.is_armed());
        data.stopped += 1;
        self.cv_notify.notify_one();
    }

    pub fn wait_in_safepoint(&self) {
        let mut data = self.data.lock().unwrap();
        assert!(data.is_armed());
        data.stopped += 1;
        self.cv_notify.notify_one();

        while data.is_armed() {
            data = self.cv_wakeup.wait(data).unwrap();
        }
    }

    pub fn wait_in_unpark(&self) {
        let mut data = self.data.lock().unwrap();

        while data.is_armed() {
            data = self.cv_wakeup.wait(data).unwrap();
        }
    }

    pub fn wait_until_threads_stopped(&self, threads: usize) {
        let mut data = self.data.lock().unwrap();
        assert!(data.is_armed());
        while data.stopped < threads {
            data = self.cv_notify.wait(data).unwrap();
        }
        assert_eq!(data.stopped, threads);
    }
}

pub struct Threads<R: Runtime> {
    pub threads: Mutex<Vec<VMThread>>,
    pub cv_join: Condvar,
    pub barrier: Barrier,
    pub next_thread_id: AtomicUsize,
    pub handshake_threads: Monitor<Vec<VMThread>, R, true>,
    marker: PhantomData<R>,
}

unsafe impl<R: Runtime> Send for Threads<R> {}
unsafe impl<R: Runtime> Sync for Threads<R> {}

impl<R: Runtime> Threads<R> {
    pub const fn new() -> Self {
        Self {
            next_thread_id: AtomicUsize::new(0),
            barrier: Barrier::new(),
            cv_join: Condvar::new(),
            threads: Mutex::new(Vec::new()),
            marker: PhantomData,
            handshake_threads: Monitor::new(Vec::new()),
        }
    }

    pub fn add_thread(&self, thread: VMThread) {
        parked_scope::<R, _, _>(|| {
            let mut threads = self.threads.lock().unwrap();
            let idx = threads.len();
            ThreadOf::<R>::set_index_in_thread_list(thread, idx);
            threads.push(thread);
        })
    }

    pub fn add_main_thread(&self, thread: VMThread) {
        let mut threads = self.threads.lock().unwrap();
        assert!(threads.is_empty());
        ThreadOf::<R>::set_index_in_thread_list(thread, 0);
        threads.push(thread);
    }
    pub fn next_thread_id(&self) -> usize {
        self.next_thread_id.fetch_add(1, Ordering::Relaxed)
    }

    pub fn remove_current_thread(&self) {
        let thread = R::current_thread();

        let _data = ThreadOf::<R>::tls(thread);

        let mut threads = self.threads.lock().unwrap();
        if !threads.contains(&thread) {
            return;
        }
        let idx = ThreadOf::<R>::index_in_thread_list(thread);
        let last = threads.pop().unwrap();

        if idx != threads.len() {
            ThreadOf::<R>::set_index_in_thread_list(last, idx);
            threads[idx] = last;
        }

        self.cv_join.notify_all();
    }
    pub fn join_all(&self) {
        let mut threads = self.threads.lock().unwrap();

        while threads.len() > 0 {
            threads = self.cv_join.wait(threads).unwrap();
        }
    }
}

pub fn parked_scope<RT: Runtime, F, R>(callback: F) -> R
where
    F: FnOnce() -> R,
{
    let thread = RT::current_thread();

    let data = ThreadOf::<RT>::tls(thread);

    let data = &*data;

    assert!(data.is_running());

    let result = callback();

    assert!(data.is_running());

    result
}

pub trait BlockAdapter<R: Runtime> {
    type BlockToken: PartialEq + Eq + Copy;
    fn is_blocked(thread: VMThread) -> bool;
    fn set_blocked(thread: VMThread, value: bool);
    fn request_block(thread: VMThread) -> Self::BlockToken;
    fn has_block_request(thread: VMThread) -> bool;
    fn has_block_request_with_token(thread: VMThread, token: Self::BlockToken) -> bool;
    fn clear_block_request(thread: VMThread);
}

/// A block adapter for GC. MMTk will use this type
/// in order to suspend all threads.
pub struct GCBlockAdapter<R: Runtime>(PhantomData<R>);

impl<R: Runtime> BlockAdapter<R> for GCBlockAdapter<R> {
    type BlockToken = ();

    fn is_blocked(thread: VMThread) -> bool {
        let tls = ThreadOf::<R>::tls(thread);

        tls.is_blocked_for_gc.load(Ordering::Relaxed)
    }

    fn set_blocked(thread: VMThread, value: bool) {
        ThreadOf::<R>::tls(thread)
            .is_blocked_for_gc
            .store(value, Ordering::Relaxed);
    }

    fn request_block(thread: VMThread) -> Self::BlockToken {
        let tls = ThreadOf::<R>::tls(thread);
        if !tls.is_blocked_for_gc.load(Ordering::Relaxed) {
            tls.should_block_for_gc.store(true, Ordering::Relaxed);
        }
    }

    fn has_block_request(thread: VMThread) -> bool {
        ThreadOf::<R>::tls(thread)
            .should_block_for_gc
            .load(Ordering::Relaxed)
    }

    fn has_block_request_with_token(thread: VMThread, token: Self::BlockToken) -> bool {
        let _ = token;
        Self::has_block_request(thread)
    }

    fn clear_block_request(thread: VMThread) {
        ThreadOf::<R>::tls(thread)
            .should_block_for_gc
            .store(false, Ordering::Relaxed);
    }
}

pub trait BlockAdapterList<R: Runtime> {
    fn acknowledge_block_requests(thread: VMThread) -> bool;
    fn is_blocked(thread: VMThread) -> bool;
}

macro_rules! block_adapter_list {
    ($(($($t: ident),*))*) => {
        $(
            impl<R: Runtime, $($t: BlockAdapter<R>),*> BlockAdapterList<R> for ($($t),*) {
                fn acknowledge_block_requests(thread: VMThread) -> bool {
                    let mut had_some = false;
                    $(
                        if $t::has_block_request(thread) {
                            $t::set_blocked(thread, true);
                            $t::clear_block_request(thread);
                            had_some = true;
                        }
                    )*

                    had_some
                }

                fn is_blocked(thread: VMThread) -> bool {
                    let mut is_blocked = false;

                    $(
                        is_blocked |= $t::is_blocked(thread);
                    )*

                    is_blocked
                }
            }

        )*
    };
}

impl<R: Runtime> BlockAdapter<R> for () {
    type BlockToken = ();
    fn clear_block_request(thread: VMThread) {
        let _ = thread;
    }

    fn has_block_request(thread: VMThread) -> bool {
        let _ = thread;
        false
    }

    fn has_block_request_with_token(thread: VMThread, token: Self::BlockToken) -> bool {
        let _ = thread;
        let _ = token;
        false
    }

    fn is_blocked(thread: VMThread) -> bool {
        let _ = thread;
        false
    }

    fn request_block(thread: VMThread) -> Self::BlockToken {
        let _ = thread;
        ()
    }

    fn set_blocked(thread: VMThread, value: bool) {
        let _ = thread;
        let _ = value;
    }
}

block_adapter_list!((X0, X1)(X0, X1, X2)(X0, X1, X2, X3)(X0, X1, X2, X3, X4)(
    X0, X1, X2, X3, X4, X5
)(X0, X1, X2, X3, X4, X5, X6)(X0, X1, X2, X3, X4, X5, X6, X7)(
    X0, X1, X2, X3, X4, X5, X6, X7, X8
)(X0, X1, X2, X3, X4, X5, X6, X7, X8, X9)(
    X0, X1, X2, X3, X4, X5, X6, X7, X8, X9, X10
)(X0, X1, X2, X3, X4, X5, X6, X7, X8, X9, X10, X11)(
    X0, X1, X2, X3, X4, X5, X6, X7, X8, X9, X10, X11, X12
)(X0, X1, X2, X3, X4, X5, X6, X7, X8, X9, X10, X11, X12, X13)(
    X0, X1, X2, X3, X4, X5, X6, X7, X8, X9, X10, X11, X12, X13, X14
)(
    X0, X1, X2, X3, X4, X5, X6, X7, X8, X9, X10, X11, X12, X13, X14, X15
)(
    X0, X1, X2, X3, X4, X5, X6, X7, X8, X9, X10, X11, X12, X13, X14, X15, X16
)(
    X0, X1, X2, X3, X4, X5, X6, X7, X8, X9, X10, X11, X12, X13, X14, X15, X16, X17
)(
    X0, X1, X2, X3, X4, X5, X6, X7, X8, X9, X10, X11, X12, X13, X14, X15, X16, X17, X18
)(
    X0, X1, X2, X3, X4, X5, X6, X7, X8, X9, X10, X11, X12, X13, X14, X15, X16, X17, X18, X19
)(
    X0, X1, X2, X3, X4, X5, X6, X7, X8, X9, X10, X11, X12, X13, X14, X15, X16, X17, X18, X19, X20
)(
    X0, X1, X2, X3, X4, X5, X6, X7, X8, X9, X10, X11, X12, X13, X14, X15, X16, X17, X18, X19, X20,
    X21
)(
    X0, X1, X2, X3, X4, X5, X6, X7, X8, X9, X10, X11, X12, X13, X14, X15, X16, X17, X18, X19, X20,
    X21, X22
)(
    X0, X1, X2, X3, X4, X5, X6, X7, X8, X9, X10, X11, X12, X13, X14, X15, X16, X17, X18, X19, X20,
    X21, X22, X23
)(
    X0, X1, X2, X3, X4, X5, X6, X7, X8, X9, X10, X11, X12, X13, X14, X15, X16, X17, X18, X19, X20,
    X21, X22, X23, X24
)(
    X0, X1, X2, X3, X4, X5, X6, X7, X8, X9, X10, X11, X12, X13, X14, X15, X16, X17, X18, X19, X20,
    X21, X22, X23, X24, X25
));

pub(crate) fn block_all_mutators_for_gc<R: Runtime>() {
    let threads = &R::vmkit().threads;

    let mut handshake = threads.handshake_threads.lock_no_handshake();

    loop {
        let actual_threads = threads.threads.lock().unwrap();

        // (1) Find all the threads that need to be blocked for GC
        for thread in actual_threads.iter() {
            if ThreadOf::<R>::is_mutator(*thread) {
                handshake.push(*thread);
            }
        }

        drop(actual_threads);

        // (2) Remove any threads that have already been blocked from the list.
        handshake.retain(|&thread| {
            let tls = ThreadOf::<R>::tls(thread);
            let guard = tls.monitor.lock_no_handshake();

            // remove if already blocked or not running
            let blocked_or_running = ThreadOf::<R>::blocked_for::<GCBlockAdapter<R>>(thread)
                || ThreadOf::<R>::block_async::<GCBlockAdapter<R>>(thread).not_running();

            drop(guard);
            !blocked_or_running
        });

        // (3) Quit trying to block threads if all threads are either blocked
        //     or not running (a thread is "not running" if it is NEW or TERMINATED;
        //     in the former case it means that the thread has not had start()
        //     called on it while in the latter case it means that the thread
        //     is either in the TERMINATED state or is about to be in that state
        //     real soon now, and will not perform any heap-related work before
        //     terminating).

        if handshake.is_empty() {
            break;
        }

        // (4) Request a block for GC from all other threads.
        while let Some(thread) = handshake.pop() {
            ThreadOf::<R>::block_sync::<GCBlockAdapter<R>>(thread);
        }
    }

    drop(handshake);
}

pub(crate) fn unblock_all_mutators_for_gc<R: Runtime>() {
    let threads = &R::vmkit().threads;

    let mut handshake = threads.handshake_threads.lock_no_handshake();
    let actual_threads = threads.threads.lock().unwrap();

    for &thread in actual_threads.iter() {
        if ThreadOf::<R>::is_mutator(thread) {
            handshake.push(thread);
        }
    }

    drop(actual_threads);

    while let Some(thread) = handshake.pop() {
        ThreadOf::<R>::unblock::<GCBlockAdapter<R>>(thread);
    }

    drop(handshake);
}

pub mod stack;

thread_local! {
    static THREAD: RefCell<VMThread> = RefCell::new(VMThread::UNINITIALIZED);
}

#[no_mangle]
pub extern "C" fn vmkit_current_thread() -> VMThread {
    THREAD.with_borrow(|thread| *thread)
}

pub extern "C" fn vmkit_get_tls<R: Runtime>() -> &'static TLSData<R> {
    let tls = THREAD.with_borrow(|thread| Address::from_ref(ThreadOf::<R>::tls(*thread)));
    unsafe { tls.as_ref() }
}

static MAIN_THREAD: AtomicUsize = AtomicUsize::new(0);

pub fn main_thread() -> VMThread {
    unsafe {
        VMThread(OpaquePointer::from_address(Address::from_usize(
            MAIN_THREAD.load(Ordering::Relaxed),
        )))
    }
}
