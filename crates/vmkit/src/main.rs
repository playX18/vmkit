use std::sync::atomic::{AtomicBool, Ordering};

use mmtk::util::Address;
use vmkit::{
    mock::{MockSuspendAdapter, MockThread, MockVM},
    runtime::{
        threads::{Thread, ThreadState},
        thunks::thread_exit,
    },
};

fn main() {
    env_logger::init();
    static STOP_SPINNING: AtomicBool = AtomicBool::new(false);

    let (handle, thread) = MockThread::spawn(
        {
            extern "C" fn x(_: u64) {
                let (handle, other_thread) = MockThread::spawn(
                    {
                        extern "C" fn y(_: u64) {
                            loop {
                                if STOP_SPINNING.load(Ordering::Relaxed) {
                                    STOP_SPINNING.store(false, Ordering::Relaxed);
                                    unsafe { thread_exit::<MockVM>(0) };
                                }

                                MockThread::check_yieldpoint(0, Address::ZERO);
                            }
                        }
                        y
                    },
                    0,
                );

                std::thread::sleep(std::time::Duration::from_millis(200));

                let state = MockThread::block_sync::<MockSuspendAdapter>(other_thread);
                STOP_SPINNING.store(true, Ordering::Relaxed);
                assert_eq!(state, ThreadState::RunningToBlock);
                MockThread::unblock::<MockSuspendAdapter>(other_thread);
                handle.unwrap().join().unwrap();
                assert!(!STOP_SPINNING.load(Ordering::Relaxed));

                unsafe { thread_exit::<MockVM>(0) }
            }
            x
        },
        0,
    );

    handle.unwrap().join().unwrap();

    MockThread::kill(thread);
}
