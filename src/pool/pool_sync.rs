/// [SyncLockPool] is an implementation of [LockPool] (see [LockPool] for API details) that can be used
/// in synchronous code. It is a little faster than [AsyncLockPool] but its locks cannot be held across
/// `await` points.
///
/// [SyncLockPool] is based on top of [std::sync::Mutex] and supports poisoning of locks.
/// See the [std::sync::Mutex] documentation for details on poisoning.
pub type SyncLockPool<K> = super::LockPoolImpl<K, std::sync::Mutex<()>>;

#[cfg(test)]
mod tests {
    use super::SyncLockPool;
    use crate::{LockPool, PoisonError, TryLockError, UnpoisonError};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    crate::instantiate_common_tests!(common, crate::SyncLockPool<isize>);

    fn poison_lock<P: LockPool<isize> + Send + Sync + 'static>(pool: &Arc<P>, key: isize) {
        let pool_ref = Arc::clone(pool);
        thread::spawn(move || {
            let _guard = pool_ref.lock(key);
            panic!("let's poison the lock");
        })
        .join()
        .expect_err("The child thread should return an error");
    }
    
    fn poison_lock_owned<P: LockPool<isize> + Send + Sync + 'static>(pool: &Arc<P>, key: isize) {
        let pool_ref = Arc::clone(pool);
        thread::spawn(move || {
            let _guard = pool_ref.lock_owned(key);
            panic!("let's poison the lock");
        })
        .join()
        .expect_err("The child thread should return an error");
    }
    
    fn poison_try_lock<P: LockPool<isize> + Send + Sync + 'static>(pool: &Arc<P>, key: isize) {
        let pool_ref = Arc::clone(pool);
        thread::spawn(move || {
            let _guard = pool_ref.try_lock(key).unwrap();
            panic!("let's poison the lock");
        })
        .join()
        .expect_err("The child thread should return an error");
    }
    
    fn poison_try_lock_owned<P: LockPool<isize> + Send + Sync + 'static>(
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
    
    fn assert_is_lock_poisoned_error<G>(expected_key: isize, error: &PoisonError<isize, G>) {
        assert_eq!(expected_key, error.key);
    }
    
    fn assert_is_try_lock_poisoned_error<G>(expected_key: isize, error: &TryLockError<isize, G>) {
        match error {
            TryLockError::Poisoned(PoisonError {
                key: actual_key, ..
            }) => {
                assert_eq!(expected_key, *actual_key);
            }
            _ => panic!("Wrong error type"),
        }
    }

    #[test]
    fn test_pool_mutex_poisoned_by_lock() {
        let pool = Arc::new(SyncLockPool::new());
    
        poison_lock(&pool, 3);
    
        // All future lock attempts should error
        {
            let err = pool.lock(3).unwrap_err();
            assert_is_lock_poisoned_error(3, &err);
        }
    
        {
            let err = pool.lock_owned(3).unwrap_err();
            assert_is_lock_poisoned_error(3, &err);
        }
    
        {
            let err = pool.try_lock(3).unwrap_err();
            assert_is_try_lock_poisoned_error(3, &err);
        }
    
        {
            let err = pool.try_lock_owned(3).unwrap_err();
            assert_is_try_lock_poisoned_error(3, &err);
        }
    
        {
            let err = pool.lock(3).unwrap_err();
            assert_is_lock_poisoned_error(3, &err);
        }
    
        {
            let err = pool.lock_owned(3).unwrap_err();
            assert_is_lock_poisoned_error(3, &err);
        }
    
        {
            let err = pool.try_lock(3).unwrap_err();
            assert_is_try_lock_poisoned_error(3, &err);
        }
    
        {
            let err = pool.try_lock_owned(3).unwrap_err();
            assert_is_try_lock_poisoned_error(3, &err);
        }
    }
    
    #[test]
    fn test_pool_mutex_poisoned_by_lock_owned() {
        let pool = Arc::new(SyncLockPool::new());
    
        poison_lock_owned(&pool, 3);
    
        // All future lock attempts should error
        {
            let err = pool.lock(3).unwrap_err();
            assert_is_lock_poisoned_error(3, &err);
        }
    
        {
            let err = pool.lock_owned(3).unwrap_err();
            assert_is_lock_poisoned_error(3, &err);
        }
    
        {
            let err = pool.try_lock(3).unwrap_err();
            assert_is_try_lock_poisoned_error(3, &err);
        }
    
        {
            let err = pool.try_lock_owned(3).unwrap_err();
            assert_is_try_lock_poisoned_error(3, &err);
        }
    
        {
            let err = pool.lock(3).unwrap_err();
            assert_is_lock_poisoned_error(3, &err);
        }
    
        {
            let err = pool.lock_owned(3).unwrap_err();
            assert_is_lock_poisoned_error(3, &err);
        }
    
        {
            let err = pool.try_lock(3).unwrap_err();
            assert_is_try_lock_poisoned_error(3, &err);
        }
    
        {
            let err = pool.try_lock_owned(3).unwrap_err();
            assert_is_try_lock_poisoned_error(3, &err);
        }
    }
    
    #[test]
    fn test_pool_mutex_poisoned_by_try_lock() {
        let pool = Arc::new(SyncLockPool::new());
    
        poison_try_lock(&pool, 3);
    
        // All future lock attempts should error
        {
            let err = pool.lock(3).unwrap_err();
            assert_is_lock_poisoned_error(3, &err);
        }
    
        {
            let err = pool.lock_owned(3).unwrap_err();
            assert_is_lock_poisoned_error(3, &err);
        }
    
        {
            let err = pool.try_lock(3).unwrap_err();
            assert_is_try_lock_poisoned_error(3, &err);
        }
    
        {
            let err = pool.try_lock_owned(3).unwrap_err();
            assert_is_try_lock_poisoned_error(3, &err);
        }
    
        {
            let err = pool.lock(3).unwrap_err();
            assert_is_lock_poisoned_error(3, &err);
        }
    
        {
            let err = pool.lock_owned(3).unwrap_err();
            assert_is_lock_poisoned_error(3, &err);
        }
    
        {
            let err = pool.try_lock(3).unwrap_err();
            assert_is_try_lock_poisoned_error(3, &err);
        }
    
        {
            let err = pool.try_lock_owned(3).unwrap_err();
            assert_is_try_lock_poisoned_error(3, &err);
        }
    }
    
    #[test]
    fn test_pool_mutex_poisoned_by_try_lock_owned() {
        let pool = Arc::new(SyncLockPool::new());
    
        poison_try_lock_owned(&pool, 3);
    
        // All future lock attempts should error
        {
            let err = pool.lock(3).unwrap_err();
            assert_is_lock_poisoned_error(3, &err);
        }
    
        {
            let err = pool.lock_owned(3).unwrap_err();
            assert_is_lock_poisoned_error(3, &err);
        }
    
        {
            let err = pool.try_lock(3).unwrap_err();
            assert_is_try_lock_poisoned_error(3, &err);
        }
    
        {
            let err = pool.try_lock_owned(3).unwrap_err();
            assert_is_try_lock_poisoned_error(3, &err);
        }
    
        {
            let err = pool.lock(3).unwrap_err();
            assert_is_lock_poisoned_error(3, &err);
        }
    
        {
            let err = pool.lock_owned(3).unwrap_err();
            assert_is_lock_poisoned_error(3, &err);
        }
    
        {
            let err = pool.try_lock(3).unwrap_err();
            assert_is_try_lock_poisoned_error(3, &err);
        }
    
        {
            let err = pool.try_lock_owned(3).unwrap_err();
            assert_is_try_lock_poisoned_error(3, &err);
        }
    }
    
    #[test]
    fn test_poison_error_holds_lock() {
        let pool = Arc::new(SyncLockPool::new());
        poison_lock(&pool, 5);
        let error = pool.lock(5).unwrap_err();
        assert_is_lock_poisoned_error(5, &error);
    
        let counter = Arc::new(AtomicU32::new(0));
    
        let child = crate::pool::tests::launch_locking_thread(&pool, 5, &counter, None);
    
        // Check that even if we wait, the child thread won't get the lock
        thread::sleep(Duration::from_millis(100));
        assert_eq!(0, counter.load(Ordering::SeqCst));
    
        // Now free the lock so the child can get it
        std::mem::drop(error);
    
        // And check that the child got it
        child.join().unwrap_err();
        assert_eq!(1, counter.load(Ordering::SeqCst));
    
        // The poisoned lock is still there
        assert_eq!(1, pool.num_locked_or_poisoned());
    }
    
    #[test]
    fn test_poison_error_holds_lock_owned() {
        let pool = Arc::new(SyncLockPool::new());
        poison_lock_owned(&pool, 5);
        let error = pool.lock_owned(5).unwrap_err();
        assert_is_lock_poisoned_error(5, &error);
    
        let counter = Arc::new(AtomicU32::new(0));
    
        let child = crate::pool::tests::launch_locking_owned_thread(&pool, 5, &counter, None);
    
        // Check that even if we wait, the child thread won't get the lock
        thread::sleep(Duration::from_millis(100));
        assert_eq!(0, counter.load(Ordering::SeqCst));
    
        // Now free the lock so the child can get it
        std::mem::drop(error);
    
        // And check that the child got it
        child.join().unwrap_err();
        assert_eq!(1, counter.load(Ordering::SeqCst));
    
        // The poisoned lock is still there
        assert_eq!(1, pool.num_locked_or_poisoned());
    }
    
    #[test]
    fn test_poison_error_holds_try_lock() {
        let pool = Arc::new(SyncLockPool::new());
        poison_lock(&pool, 5);
        let error = pool.lock(5).unwrap_err();
        assert_is_lock_poisoned_error(5, &error);
    
        let err = pool.try_lock(5).unwrap_err();
        assert!(matches!(err, TryLockError::WouldBlock));
    
        // Now free the lock so the child can get it
        std::mem::drop(error);
    
        // And check that it is still poisoned
        let err = pool.try_lock(5).unwrap_err();
        assert_is_try_lock_poisoned_error(5, &err);
    
        // The poisoned lock is still there
        assert_eq!(1, pool.num_locked_or_poisoned());
    }
    
    #[test]
    fn test_poison_error_holds_try_lock_owned() {
        let pool = Arc::new(SyncLockPool::new());
        poison_lock_owned(&pool, 5);
        let error = pool.lock_owned(5).unwrap_err();
        assert_is_lock_poisoned_error(5, &error);
    
        let err = pool.try_lock_owned(5).unwrap_err();
        assert!(matches!(err, TryLockError::WouldBlock));
    
        // Now free the lock so the child can get it
        std::mem::drop(error);
    
        // And check that it is still poisoned
        let err = pool.try_lock(5).unwrap_err();
        assert_is_try_lock_poisoned_error(5, &err);
    
        // The poisoned lock is still there
        assert_eq!(1, pool.num_locked_or_poisoned());
    }
    
    #[test]
    fn test_unpoison() {
        let pool = Arc::new(SyncLockPool::new());
    
        poison_lock(&pool, 3);
    
        // Check that it is actually poisoned
        {
            let err = pool.lock(3).unwrap_err();
            assert_is_lock_poisoned_error(3, &err);
        }
    
        pool.unpoison(3).unwrap();
    
        // Check that it got unpoisoned
        {
            let _g = pool.lock(3).unwrap();
        }
        {
            let _g = pool.lock_owned(3).unwrap();
        }
        {
            let _g = pool.try_lock(3).unwrap();
        }
        {
            let _g = pool.try_lock_owned(3).unwrap();
        }
    }
    
    #[test]
    fn test_unpoison_not_poisoned() {
        let pool = Arc::new(SyncLockPool::new());
    
        let err = pool.unpoison(3).unwrap_err();
        assert_eq!(UnpoisonError::NotPoisoned, err);
    }
    
    #[test]
    fn test_unpoison_while_other_thread_waiting() {
        let pool = Arc::new(SyncLockPool::new());
    
        poison_lock(&pool, 3);
    
        let _err_guard = pool.lock(3).unwrap_err();
    
        // _err_guard keeps it locked
    
        let err = pool.unpoison(3).unwrap_err();
        assert!(matches!(err, UnpoisonError::OtherThreadsBlockedOnMutex));
    }
}
