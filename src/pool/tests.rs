//! This module contains some test cases that are common between [SyncLockPool] and [TokioLockPool]

use super::LockPool;
use crate::TryLockError;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub mod utils {
    use crate::LockPool;
    #[cfg(feature = "tokio")]
    use crate::pool::pool_async::AsyncLockPool;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread::{self, JoinHandle};

    // Launch a thread that
    // 1. locks the given key
    // 2. once it has the lock, increments a counter
    // 3. then waits until a barrier is released before it releases the lock
    pub fn launch_locking_thread<P: LockPool<isize> + Send + Sync + 'static>(
        pool: &Arc<P>,
        key: isize,
        counter: &Arc<AtomicU32>,
        barrier: Option<&Arc<Mutex<()>>>,
    ) -> JoinHandle<()> {
        let pool = Arc::clone(pool);
        let counter = Arc::clone(counter);
        let barrier = barrier.map(Arc::clone);
        thread::spawn(move || {
            let guard = pool.lock(key);
            counter.fetch_add(1, Ordering::SeqCst);
            let _guard = guard.unwrap();
            if let Some(barrier) = barrier {
                let _barrier = barrier.lock().unwrap();
            }
        })
    }

    #[cfg(feature = "tokio")]
    pub fn launch_locking_async_thread<P: AsyncLockPool<isize> + Send + Sync + 'static>(
        pool: &Arc<P>,
        key: isize,
        counter: &Arc<AtomicU32>,
        barrier: Option<&Arc<Mutex<()>>>,
    ) -> JoinHandle<()> {
        let pool = Arc::clone(pool);
        let counter = Arc::clone(counter);
        let barrier = barrier.map(Arc::clone);
        thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            let _guard = runtime.block_on(pool.lock_async(key));
            counter.fetch_add(1, Ordering::SeqCst);
            if let Some(barrier) = barrier {
                let _barrier = barrier.lock().unwrap();
            }
        })
    }

    pub fn launch_locking_owned_thread<P: LockPool<isize> + Send + Sync + 'static>(
        pool: &Arc<P>,
        key: isize,
        counter: &Arc<AtomicU32>,
        barrier: Option<&Arc<Mutex<()>>>,
    ) -> JoinHandle<()> {
        let pool = Arc::clone(pool);
        let counter = Arc::clone(counter);
        let barrier = barrier.map(Arc::clone);
        thread::spawn(move || {
            let guard = pool.lock_owned(key);
            counter.fetch_add(1, Ordering::SeqCst);
            let _guard = guard.unwrap();
            if let Some(barrier) = barrier {
                let _barrier = barrier.lock().unwrap();
            }
        })
    }

    #[cfg(feature = "tokio")]
    pub fn launch_locking_owned_async_thread<P: AsyncLockPool<isize> + Send + Sync + 'static>(
        pool: &Arc<P>,
        key: isize,
        counter: &Arc<AtomicU32>,
        barrier: Option<&Arc<Mutex<()>>>,
    ) -> JoinHandle<()> {
        let pool = Arc::clone(pool);
        let counter = Arc::clone(counter);
        let barrier = barrier.map(Arc::clone);
        thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            let _guard = runtime.block_on(pool.lock_owned_async(key));
            counter.fetch_add(1, Ordering::SeqCst);
            if let Some(barrier) = barrier {
                let _barrier = barrier.lock().unwrap(); 
            }
        })
    }

    pub fn poison_lock<P: LockPool<isize> + Send + Sync + 'static>(pool: &Arc<P>, key: isize) {
        let pool_ref = Arc::clone(pool);
        thread::spawn(move || {
            let _guard = pool_ref.lock(key);
            panic!("let's poison the lock");
        })
        .join()
        .expect_err("The child thread should return an error");
    }

    pub fn poison_lock_owned<P: LockPool<isize> + Send + Sync + 'static>(
        pool: &Arc<P>,
        key: isize,
    ) {
        let pool_ref = Arc::clone(pool);
        thread::spawn(move || {
            let _guard = pool_ref.lock_owned(key);
            panic!("let's poison the lock");
        })
        .join()
        .expect_err("The child thread should return an error");
    }

    pub fn poison_try_lock<P: LockPool<isize> + Send + Sync + 'static>(pool: &Arc<P>, key: isize) {
        let pool_ref = Arc::clone(pool);
        thread::spawn(move || {
            let _guard = pool_ref.try_lock(key).unwrap();
            panic!("let's poison the lock");
        })
        .join()
        .expect_err("The child thread should return an error");
    }

    pub fn poison_try_lock_owned<P: LockPool<isize> + Send + Sync + 'static>(
        pool: &Arc<P>,
        key: isize,
    ) {
        let pool_ref = Arc::clone(pool);
        thread::spawn(move || {
            let _guard = pool_ref.try_lock_owned(key).unwrap();
            panic!("let's poison the lock");
        })
        .join()
        .expect_err("The child thread should return an error");
    }
}

use utils::{launch_locking_owned_thread, launch_locking_thread};

pub fn test_simple_lock_unlock<P: LockPool<isize>>() {
    let pool = P::new();
    assert_eq!(0, pool.num_locked_or_poisoned());
    let guard = pool.lock(4).unwrap();
    assert_eq!(1, pool.num_locked_or_poisoned());
    std::mem::drop(guard);
    assert_eq!(0, pool.num_locked_or_poisoned());
}

pub fn test_simple_lock_owned_unlock<P: LockPool<isize>>() {
    let pool = Arc::new(P::new());
    assert_eq!(0, pool.num_locked_or_poisoned());
    let guard = pool.lock_owned(4).unwrap();
    assert_eq!(1, pool.num_locked_or_poisoned());
    std::mem::drop(guard);
    assert_eq!(0, pool.num_locked_or_poisoned());
}

pub fn test_simple_try_lock_unlock<P: LockPool<isize>>() {
    let pool = P::new();
    assert_eq!(0, pool.num_locked_or_poisoned());
    let guard = pool.try_lock(4).unwrap();
    assert_eq!(1, pool.num_locked_or_poisoned());
    std::mem::drop(guard);
    assert_eq!(0, pool.num_locked_or_poisoned());
}

pub fn test_simple_try_lock_owned_unlock<P: LockPool<isize>>() {
    let pool = Arc::new(P::new());
    assert_eq!(0, pool.num_locked_or_poisoned());
    let guard = pool.try_lock_owned(4).unwrap();
    assert_eq!(1, pool.num_locked_or_poisoned());
    std::mem::drop(guard);
    assert_eq!(0, pool.num_locked_or_poisoned());
}

pub fn test_multi_lock_unlock<P: LockPool<isize>>() {
    let pool = P::new();
    assert_eq!(0, pool.num_locked_or_poisoned());
    let guard1 = pool.lock(1).unwrap();
    assert_eq!(1, pool.num_locked_or_poisoned());
    let guard2 = pool.lock(2).unwrap();
    assert_eq!(2, pool.num_locked_or_poisoned());
    let guard3 = pool.lock(3).unwrap();
    assert_eq!(3, pool.num_locked_or_poisoned());

    std::mem::drop(guard2);
    assert_eq!(2, pool.num_locked_or_poisoned());
    std::mem::drop(guard1);
    assert_eq!(1, pool.num_locked_or_poisoned());
    std::mem::drop(guard3);
    assert_eq!(0, pool.num_locked_or_poisoned());
}

pub fn test_multi_lock_owned_unlock<P: LockPool<isize>>() {
    let pool = Arc::new(P::new());
    assert_eq!(0, pool.num_locked_or_poisoned());
    let guard1 = pool.lock_owned(1).unwrap();
    assert_eq!(1, pool.num_locked_or_poisoned());
    let guard2 = pool.lock_owned(2).unwrap();
    assert_eq!(2, pool.num_locked_or_poisoned());
    let guard3 = pool.lock_owned(3).unwrap();
    assert_eq!(3, pool.num_locked_or_poisoned());

    std::mem::drop(guard2);
    assert_eq!(2, pool.num_locked_or_poisoned());
    std::mem::drop(guard1);
    assert_eq!(1, pool.num_locked_or_poisoned());
    std::mem::drop(guard3);
    assert_eq!(0, pool.num_locked_or_poisoned());
}

pub fn test_multi_try_lock_unlock<P: LockPool<isize>>() {
    let pool = P::new();
    assert_eq!(0, pool.num_locked_or_poisoned());
    let guard1 = pool.try_lock(1).unwrap();
    assert_eq!(1, pool.num_locked_or_poisoned());
    let guard2 = pool.try_lock(2).unwrap();
    assert_eq!(2, pool.num_locked_or_poisoned());
    let guard3 = pool.try_lock(3).unwrap();
    assert_eq!(3, pool.num_locked_or_poisoned());

    std::mem::drop(guard2);
    assert_eq!(2, pool.num_locked_or_poisoned());
    std::mem::drop(guard1);
    assert_eq!(1, pool.num_locked_or_poisoned());
    std::mem::drop(guard3);
    assert_eq!(0, pool.num_locked_or_poisoned());
}

pub fn test_multi_try_lock_owned_unlock<P: LockPool<isize>>() {
    let pool = Arc::new(P::new());
    assert_eq!(0, pool.num_locked_or_poisoned());
    let guard1 = pool.try_lock_owned(1).unwrap();
    assert_eq!(1, pool.num_locked_or_poisoned());
    let guard2 = pool.try_lock_owned(2).unwrap();
    assert_eq!(2, pool.num_locked_or_poisoned());
    let guard3 = pool.try_lock_owned(3).unwrap();
    assert_eq!(3, pool.num_locked_or_poisoned());

    std::mem::drop(guard2);
    assert_eq!(2, pool.num_locked_or_poisoned());
    std::mem::drop(guard1);
    assert_eq!(1, pool.num_locked_or_poisoned());
    std::mem::drop(guard3);
    assert_eq!(0, pool.num_locked_or_poisoned());
}

pub fn test_concurrent_lock<P: LockPool<isize> + Send + Sync + 'static>() {
    let pool = Arc::new(P::new());
    let guard = pool.lock(5).unwrap();

    let counter = Arc::new(AtomicU32::new(0));

    let child = launch_locking_thread(&pool, 5, &counter, None);

    // Check that even if we wait, the child thread won't get the lock
    thread::sleep(Duration::from_millis(100));
    assert_eq!(0, counter.load(Ordering::SeqCst));

    // Check that we can stil lock other locks while the child is waiting
    {
        let _g = pool.lock(4).unwrap();
    }

    // Now free the lock so the child can get it
    std::mem::drop(guard);

    // And check that the child got it
    child.join().unwrap();
    assert_eq!(1, counter.load(Ordering::SeqCst));

    assert_eq!(0, pool.num_locked_or_poisoned());
}

pub fn test_concurrent_lock_owned<P: LockPool<isize> + Send + Sync + 'static>() {
    let pool = Arc::new(P::new());
    let guard = pool.lock_owned(5).unwrap();

    let counter = Arc::new(AtomicU32::new(0));

    let child = launch_locking_owned_thread(&pool, 5, &counter, None);

    // Check that even if we wait, the child thread won't get the lock
    thread::sleep(Duration::from_millis(100));
    assert_eq!(0, counter.load(Ordering::SeqCst));

    // Check that we can stil lock other locks while the child is waiting
    {
        let _g = pool.lock_owned(4).unwrap();
    }

    // Now free the lock so the child can get it
    std::mem::drop(guard);

    // And check that the child got it
    child.join().unwrap();
    assert_eq!(1, counter.load(Ordering::SeqCst));

    assert_eq!(0, pool.num_locked_or_poisoned());
}

pub fn test_concurrent_try_lock<P: LockPool<isize>>() {
    let pool = Arc::new(P::new());
    let guard = pool.lock(5).unwrap();

    let error = pool.try_lock(5).unwrap_err();
    assert!(matches!(error, TryLockError::WouldBlock));

    // Check that we can stil lock other locks while the child is waiting
    {
        let _g = pool.try_lock(4).unwrap();
    }

    // Now free the lock so the we can get it again
    std::mem::drop(guard);

    // And check that we can get it again
    {
        let _g = pool.try_lock(5).unwrap();
    }

    assert_eq!(0, pool.num_locked_or_poisoned());
}

pub fn test_concurrent_try_lock_owned<P: LockPool<isize>>() {
    let pool = Arc::new(P::new());
    let guard = pool.lock_owned(5).unwrap();

    let error = pool.try_lock_owned(5).unwrap_err();
    assert!(matches!(error, TryLockError::WouldBlock));

    // Check that we can stil lock other locks while the child is waiting
    {
        let _g = pool.try_lock_owned(4).unwrap();
    }

    // Now free the lock so the we can get it again
    std::mem::drop(guard);

    // And check that we can get it again
    {
        let _g = pool.try_lock_owned(5).unwrap();
    }

    assert_eq!(0, pool.num_locked_or_poisoned());
}

pub fn test_multi_concurrent_lock<P: LockPool<isize> + Send + Sync + 'static>() {
    let pool = Arc::new(P::new());
    let guard = pool.lock(5).unwrap();

    let counter = Arc::new(AtomicU32::new(0));
    let barrier = Arc::new(Mutex::new(()));
    let barrier_guard = barrier.lock().unwrap();

    let child1 = launch_locking_thread(&pool, 5, &counter, Some(&barrier));
    let child2 = launch_locking_thread(&pool, 5, &counter, Some(&barrier));

    // Check that even if we wait, the child thread won't get the lock
    thread::sleep(Duration::from_millis(100));
    assert_eq!(0, counter.load(Ordering::SeqCst));

    // Check that we can stil lock other locks while the children are waiting
    {
        let _g = pool.lock(4).unwrap();
    }

    // Now free the lock so a child can get it
    std::mem::drop(guard);

    // Check that a child got it
    thread::sleep(Duration::from_millis(100));
    assert_eq!(1, counter.load(Ordering::SeqCst));

    // Allow the child to free the lock
    std::mem::drop(barrier_guard);

    // Check that the other child got it
    child1.join().unwrap();
    child2.join().unwrap();
    assert_eq!(2, counter.load(Ordering::SeqCst));

    assert_eq!(0, pool.num_locked_or_poisoned());
}

pub fn test_multi_concurrent_lock_owned<P: LockPool<isize> + Send + Sync + 'static>() {
    let pool = Arc::new(P::new());
    let guard = pool.lock_owned(5).unwrap();

    let counter = Arc::new(AtomicU32::new(0));
    let barrier = Arc::new(Mutex::new(()));
    let barrier_guard = barrier.lock().unwrap();

    let child1 = launch_locking_owned_thread(&pool, 5, &counter, Some(&barrier));
    let child2 = launch_locking_owned_thread(&pool, 5, &counter, Some(&barrier));

    // Check that even if we wait, the child thread won't get the lock
    thread::sleep(Duration::from_millis(100));
    assert_eq!(0, counter.load(Ordering::SeqCst));

    // Check that we can stil lock other locks while the children are waiting
    {
        let _g = pool.lock_owned(4).unwrap();
    }

    // Now free the lock so a child can get it
    std::mem::drop(guard);

    // Check that a child got it
    thread::sleep(Duration::from_millis(100));
    assert_eq!(1, counter.load(Ordering::SeqCst));

    // Allow the child to free the lock
    std::mem::drop(barrier_guard);

    // Check that the other child got it
    child1.join().unwrap();
    child2.join().unwrap();
    assert_eq!(2, counter.load(Ordering::SeqCst));

    assert_eq!(0, pool.num_locked_or_poisoned());
}

pub fn test_lock_owned_guards_can_be_passed_around<P: LockPool<isize> + Send + Sync + 'static>() {
    let make_guard = || {
        let pool = Arc::new(P::new());
        pool.lock_owned(5)
    };
    let _guard = make_guard();
}

pub fn test_try_lock_owned_guards_can_be_passed_around<
    P: LockPool<isize> + Send + Sync + 'static,
>() {
    let make_guard = || {
        let pool = Arc::new(P::new());
        pool.try_lock_owned(5)
    };
    let guard = make_guard();
    assert!(guard.is_ok());
}

#[macro_export]
macro_rules! instantiate_common_tests {
    (@impl, $lock_pool:ty, $test_name:ident) => {
        #[test]
        fn $test_name() {
            $crate::pool::tests::$test_name::<$lock_pool>();
        }
    };
    ($type_name: ident, $lock_pool:ty) => {
        mod $type_name {
            // TODO There's still lots of duplication between the normal and the _owned tests.
            //      Can we deduplicate this similar to how we deduplicated TokioLockPool vs SyncLockPool tests with a macro here?
            $crate::instantiate_common_tests!(@impl, $lock_pool, test_simple_lock_unlock);
            $crate::instantiate_common_tests!(@impl, $lock_pool, test_simple_lock_owned_unlock);
            $crate::instantiate_common_tests!(@impl, $lock_pool, test_simple_try_lock_unlock);
            $crate::instantiate_common_tests!(@impl, $lock_pool, test_simple_try_lock_owned_unlock);
            $crate::instantiate_common_tests!(@impl, $lock_pool, test_multi_lock_unlock);
            $crate::instantiate_common_tests!(@impl, $lock_pool, test_multi_lock_owned_unlock);
            $crate::instantiate_common_tests!(@impl, $lock_pool, test_multi_try_lock_unlock);
            $crate::instantiate_common_tests!(@impl, $lock_pool, test_multi_try_lock_owned_unlock);
            $crate::instantiate_common_tests!(@impl, $lock_pool, test_concurrent_lock);
            $crate::instantiate_common_tests!(@impl, $lock_pool, test_concurrent_lock_owned);
            $crate::instantiate_common_tests!(@impl, $lock_pool, test_concurrent_try_lock);
            $crate::instantiate_common_tests!(@impl, $lock_pool, test_concurrent_try_lock_owned);
            $crate::instantiate_common_tests!(@impl, $lock_pool, test_multi_concurrent_lock);
            $crate::instantiate_common_tests!(@impl, $lock_pool, test_multi_concurrent_lock_owned);
            $crate::instantiate_common_tests!(@impl, $lock_pool, test_lock_owned_guards_can_be_passed_around);
            $crate::instantiate_common_tests!(@impl, $lock_pool, test_try_lock_owned_guards_can_be_passed_around);
        }
    };
}
