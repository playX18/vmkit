//! Simple MockVM used in tests

use std::{
    mem::offset_of,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        LazyLock,
    },
};

use mmtk::{
    util::{Address, OpaquePointer, VMMutatorThread, VMThread},
    vm::{slot::SimpleSlot, RootsWorkFactory},
};

use crate::{
    objectmodel::vtable::GCVTable,
    runtime::threads::{BlockAdapter, GCBlockAdapter, TLSData, Thread},
    Runtime, VMKit, VMKitBuilder,
};

#[derive(Default)]
pub struct MockVM;

impl Runtime for MockVM {
    type Slot = SimpleSlot;
    type VTable = GCVTable<Self>;
    type Thread = MockThread;

    fn out_of_memory(_thread: VMThread, _error: mmtk::util::alloc::AllocationError) {}

    fn scan_roots(_roots: impl mmtk::vm::RootsWorkFactory<Self::Slot>) {}

    fn vmkit() -> &'static crate::VMKit<Self> {
        &VMKIT
    }

    fn post_forwarding() {}

    fn stack_overflow(_ip: Address, _addr: Address) -> ! {
        loop {}
    }
    fn null_pointer_access(_ip: Address) -> ! {
        loop {}
    }
}

static VMKIT: LazyLock<VMKit<MockVM>> =
    LazyLock::new(|| VMKitBuilder::new().from_options().build());

pub struct MockThread {
    tls: TLSData<MockVM>,
    mock_suspend_token: AtomicUsize,
    mock_block_requested: AtomicBool,
    mock_blocked: AtomicBool,
}

impl From<VMThread> for &MockThread {
    fn from(value: VMThread) -> Self {
        unsafe { value.0.to_address().as_ref() }
    }
}
impl From<VMMutatorThread> for &MockThread {
    fn from(value: VMMutatorThread) -> Self {
        unsafe { value.0 .0.to_address().as_ref() }
    }
}

impl Thread<MockVM> for MockThread {
    type BlockAdapterList = (GCBlockAdapter<MockVM>, MockSuspendAdapter);

    const TLS_OFFSET: Option<usize> = Some(offset_of!(Self, tls));

    fn new(tls: TLSData<MockVM>) -> VMThread {
        VMThread(OpaquePointer::from_address(Address::from_mut_ptr(
            Box::into_raw(Box::new(Self {
                tls,
                mock_block_requested: AtomicBool::new(false),
                mock_suspend_token: AtomicUsize::new(0),
                mock_blocked: AtomicBool::new(false),
            })),
        )))
    }

    fn id(thread: VMThread) -> u64 {
        thread.0.to_address().as_usize() as _
    }

    fn tls<'a>(thread: VMThread) -> &'a TLSData<MockVM> {
        unsafe { &thread.0.to_address().as_ref::<Self>().tls }
    }

    fn scan_roots(_thread: VMMutatorThread, _factory: impl RootsWorkFactory<SimpleSlot>) {}

    fn save_thread_state() {}
}

impl MockThread {
    pub fn kill(thread: VMThread) {
        unsafe {
            let _ = Box::from_raw(thread.0.to_address().to_mut_ptr::<Self>());
        }
    }
}

pub struct MockSuspendAdapter;

impl BlockAdapter<MockVM> for MockSuspendAdapter {
    type BlockToken = usize;
    fn clear_block_request(thread: VMThread) {
        let mock: &MockThread = From::from(thread);

        mock.mock_block_requested
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }

    fn has_block_request(thread: VMThread) -> bool {
        let mock: &MockThread = From::from(thread);

        mock.mock_block_requested
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    fn has_block_request_with_token(thread: VMThread, token: Self::BlockToken) -> bool {
        let mock: &MockThread = From::from(thread);

        mock.mock_block_requested
            .load(std::sync::atomic::Ordering::Relaxed)
            && mock
                .mock_suspend_token
                .load(std::sync::atomic::Ordering::Relaxed)
                == token
    }

    fn is_blocked(thread: VMThread) -> bool {
        let mock: &MockThread = From::from(thread);

        mock.mock_blocked.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn request_block(thread: VMThread) -> Self::BlockToken {
        let mock: &MockThread = From::from(thread);

        if mock.mock_blocked.load(Ordering::Relaxed)
            || mock.mock_block_requested.load(Ordering::Relaxed)
        {
            return mock.mock_suspend_token.load(Ordering::Relaxed);
        } else {
            mock.mock_block_requested.store(true, Ordering::Relaxed);
            mock.mock_suspend_token.fetch_add(1, Ordering::Relaxed) + 1
        }
    }

    fn set_blocked(thread: VMThread, value: bool) {
        let mock: &MockThread = From::from(thread);

        mock.mock_blocked
            .store(value, std::sync::atomic::Ordering::Relaxed);
    }
}
