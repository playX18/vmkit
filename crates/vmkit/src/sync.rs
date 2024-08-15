use crate::threads::parked_scope;
use crate::Runtime;
use crate::{threads::Thread, ThreadOf};
use parking_lot::{lock_api::RawMutex, Condvar, Mutex, MutexGuard};
use std::ops::{Deref, DerefMut};
use std::{
    marker::PhantomData,
    sync::atomic::{AtomicU64, AtomicUsize, Ordering},
    u64,
};

/// A monitor is mechanism to control concurrent access to an object.
///
/// This type is implemented on top of regular mutex + condvar and also
/// can function as a recursive mutex. On it's own this type is quite "heavy"
/// as it is around 32 bytes in size by default. In case you need to store
/// lock per object we provide a separate API that tries to use bits in object header first.
pub struct Monitor<T, R: Runtime, const SAFEPOINT: bool = true> {
    lock: Mutex<T>,
    cvar: Condvar,
    holder: AtomicU64,
    rec_count: AtomicUsize,
    marker: PhantomData<R>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct RecCount(usize);

impl RecCount {
    pub fn value(&self) -> usize {
        self.0
    }
}

impl<T, R: Runtime, const SAFEPOINT: bool> Monitor<T, R, SAFEPOINT> {
    pub const fn new(value: T) -> Self {
        Self {
            marker: PhantomData,
            lock: Mutex::new(value),
            cvar: Condvar::new(),
            holder: AtomicU64::new(u64::MAX),
            rec_count: AtomicUsize::new(0),
        }
    }

    pub unsafe fn unlock_completely<'a>(
        guard: MonitorGuard<'a, T, R, SAFEPOINT>,
    ) -> (RecCount, &Self) {
        let rec_count = guard.monitor.rec_count.swap(0, Ordering::Relaxed);
        guard.monitor.holder.store(u64::MAX, Ordering::Relaxed);
        unsafe {
            guard.monitor.lock.raw().unlock();
        }
        (RecCount(rec_count), guard.monitor)
    }

    /// Relock monitor with previous recursive count associated with it.
    ///
    /// # Safety
    ///
    /// User must verify that `rec_count` is a rec count from previously called [unlock_completely](Monitor::unlock_completely).
    pub unsafe fn relock_no_handshake<'a>(
        &'a self,
        rec_count: RecCount,
    ) -> MonitorGuard<'a, T, R, SAFEPOINT> {
        let guard = self.lock.lock();

        self.rec_count.store(rec_count.0, Ordering::Relaxed);
        self.holder
            .store(ThreadOf::<R>::id(R::current_thread()), Ordering::Relaxed);

        MonitorGuard {
            guard: Some(guard),
            monitor: self,
        }
    }

    pub fn lock_no_handshake<'a>(&'a self) -> MonitorGuard<'a, T, R, SAFEPOINT> {
        let my_slot = ThreadOf::<R>::id(R::current_thread());
        if my_slot != self.holder.load(Ordering::Relaxed) {
            let guard = self.lock.lock();
            self.holder.store(my_slot, Ordering::Relaxed);
            self.rec_count.fetch_add(1, Ordering::Relaxed);
            return MonitorGuard {
                guard: Some(guard),
                monitor: self,
            };
        } else {
            let guard = MonitorGuard {
                guard: unsafe { Some(self.lock.make_guard_unchecked()) },
                monitor: self,
            };

            guard.monitor.rec_count.fetch_add(1, Ordering::Relaxed);

            guard
        }
    }

    pub fn lock_with_handshake(&self) -> MonitorGuard<'_, T, R, SAFEPOINT> {
        let my_slot = ThreadOf::<R>::id(R::current_thread());
        if my_slot != self.holder.load(Ordering::Relaxed) {
            let guard = self.lock_with_handshake_no_rec();
            self.holder.store(my_slot, Ordering::Relaxed);
            self.rec_count.fetch_add(1, Ordering::Relaxed);
            return guard;
        }

        self.rec_count.fetch_add(1, Ordering::Relaxed);

        MonitorGuard {
            guard: unsafe { Some(Mutex::make_guard_unchecked(&self.lock)) },
            monitor: self,
        }
    }

    pub fn relock_with_handshake(&self, rec_count: RecCount) -> MonitorGuard<'_, T, R, SAFEPOINT> {
        ThreadOf::<R>::save_thread_state();
        let guard = loop {
            ThreadOf::<R>::enter_parked();
            let guard = self.lock.lock();

            if ThreadOf::<R>::attempt_leave_parked_no_block() {
                break MonitorGuard {
                    guard: Some(guard),
                    monitor: self,
                };
            }

            drop(guard);
            ThreadOf::<R>::leave_parked();
        };

        guard
            .monitor
            .holder
            .store(ThreadOf::<R>::id(R::current_thread()), Ordering::Relaxed);
        guard
            .monitor
            .rec_count
            .store(rec_count.0, Ordering::Relaxed);

        guard
    }

    fn lock_with_handshake_no_rec(&self) -> MonitorGuard<'_, T, R, SAFEPOINT> {
        ThreadOf::<R>::save_thread_state();
        loop {
            ThreadOf::<R>::enter_parked();
            let guard = self.lock.lock();

            if ThreadOf::<R>::attempt_leave_parked_no_block() {
                return MonitorGuard {
                    guard: Some(guard),
                    monitor: self,
                };
            } else {
                drop(guard);
                ThreadOf::<R>::leave_parked()
            }
        }
    }

    pub fn notify_all(&self) {
        self.cvar.notify_all();
    }

    pub fn notify_one(&self) {
        self.cvar.notify_one();
    }
}

pub struct MonitorGuard<'a, T, R: Runtime, const SAFEPOINT: bool> {
    pub guard: Option<MutexGuard<'a, T>>,
    pub monitor: &'a Monitor<T, R, SAFEPOINT>,
}

impl<'a, T, R: Runtime, const SAFEPOINT: bool> MonitorGuard<'a, T, R, SAFEPOINT> {
    pub fn leak(mut guard: Self) -> &'a mut T {
        MutexGuard::leak(guard.guard.take().expect("impossible"))
    }

    pub fn wait_no_handshake(&mut self) {
        let rec_count = self.monitor.rec_count.swap(0, Ordering::Relaxed);
        self.monitor.holder.store(u64::MAX, Ordering::Relaxed);

        self.monitor.cvar.wait(&mut self.guard.as_mut().unwrap());

        self.monitor.rec_count.store(rec_count, Ordering::Relaxed);
        self.monitor
            .holder
            .store(ThreadOf::<R>::id(R::current_thread()), Ordering::Relaxed);
    }

    pub fn wait_with_handshake(self) -> Self {
        ThreadOf::<R>::save_thread_state();
        self.wait_with_handshake_impl()
    }

    #[inline(never)]
    fn wait_with_handshake_impl(mut self) -> Self {
        let (rec_count, mon) = parked_scope::<R, _, _>(|| unsafe {
            self.wait_no_handshake();
            let (rec_count, mon) = Monitor::unlock_completely(self);

            (rec_count, mon)
        });

        Monitor::relock_with_handshake(mon, rec_count)
    }
}

impl<'a, T, R: Runtime, const SAFEPOINT: bool> Drop for MonitorGuard<'a, T, R, SAFEPOINT> {
    fn drop(&mut self) {
        let Some(guard) = self.guard.take() else {
            unreachable!()
        };

        if self.monitor.rec_count.fetch_sub(1, Ordering::Relaxed) == 1 {
            drop(guard);
        } else {
            MutexGuard::leak(guard);
        }
    }
}

impl<'a, T, R: Runtime, const SAFEPOINT: bool> Deref for MonitorGuard<'a, T, R, SAFEPOINT> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &*self.guard.as_ref().unwrap()
    }
}

impl<'a, T, R: Runtime, const SAFEPOINT: bool> DerefMut for MonitorGuard<'a, T, R, SAFEPOINT> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.guard.as_mut().unwrap()
    }
}
