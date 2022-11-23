use async_trait::async_trait;
use owning_ref_lockable::OwningHandle;
use std::fmt::Debug;
use std::hash::Hash;
use std::ops::Deref;
use std::sync::Arc;

use crate::guard::GuardImpl;
use crate::pool::LockPool;

/// [TokioLockPool] is an implementation of [AsyncLockPool] (see [AsyncLockPool] for API details) and is based
/// on top of [tokio::sync::Mutex]. This means the lock pool can be used in asynchronous code and its locks
/// can be held across `await` points. It is a little slower than [SyncLockPool].
///
/// This lock pool is only available if the `tokio` crate feature is enabled.
///
/// This implementation can also be used in synchronous code since it also implements the [LockPool] API,
/// but it will panic if you call [LockPool::lock] or [LockPool::lock_owned] from an `async` context,
/// see the documentation of [tokio::sync::Mutex::blocking_lock].
///
/// [TokioLockPool] is based on top of [tokio::sync::Mutex] and does not support poisoning of locks.
/// See the [tokio::sync::Mutex] documentation for details on poisoning.
#[cfg(feature = "tokio")]
pub type TokioLockPool<K> = super::LockPoolImpl<K, tokio::sync::Mutex<()>>;

/// TODO
#[async_trait]
pub trait AsyncLockPool<K>: LockPool<K>
where
    K: Eq + PartialEq + Hash + Clone + Debug + Send,
{
    /// TODO
    async fn lock_async(&self, key: K) -> Self::Guard<'_>;

    /// TODO
    async fn lock_owned_async<'a>(self: &'a Arc<Self>, key: K) -> Self::OwnedGuard;
}

#[async_trait]
impl<K> AsyncLockPool<K> for TokioLockPool<K>
where
    K: Eq + PartialEq + Hash + Clone + Debug + Send + 'static,
{
    async fn lock_async(&self, key: K) -> Self::Guard<'_> {
        Self::_lock_async(self, key).await
    }

    async fn lock_owned_async<'a>(self: &'a Arc<Self>, key: K) -> Self::OwnedGuard {
        Self::_lock_async(Arc::clone(self), key).await
    }
}

impl<K> TokioLockPool<K>
where
    K: Eq + PartialEq + Hash + Clone + Debug + Send + 'static,
{
    async fn _lock_async<'a, S: 'a + Deref<Target = Self>>(
        this: S,
        key: K,
    ) -> GuardImpl<'a, K, tokio::sync::Mutex<()>, S> {
        let mutex = this._load_or_insert_mutex_for_key(&key);
        // Now we have an Arc::clone of the mutex for this key, and the global mutex is already unlocked so other threads can access the hash map.
        // The following blocks until the mutex for this key is acquired.

        let guard =
            OwningHandle::new_with_async_fn(mutex, |mutex: *const tokio::sync::Mutex<()>| {
                let mutex: &tokio::sync::Mutex<()> = unsafe { &*mutex };
                mutex.lock()
            })
            .await;
        GuardImpl::new(this, key, guard, false)
    }
}

#[cfg(test)]
mod tests {
    use super::{AsyncLockPool, TokioLockPool};
    use crate::pool::tests::utils::{
        launch_locking_async_thread, launch_locking_owned_async_thread,
    };
    use crate::LockPool;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    // TODO Add tests checking that the lock_async, lock_owned, lock methods all block each other. For lock and lock_owned that can probably go into common tests.rs

    crate::instantiate_common_tests!(common, super::TokioLockPool<isize>);

    fn poison_lock<P: LockPool<isize> + Send + Sync + 'static>(pool: &Arc<P>, key: isize) {
        let pool_ref = Arc::clone(pool);
        thread::spawn(move || {
            let _guard = pool_ref.lock(key);
            panic!("let's poison the lock");
        })
        .join()
        .expect_err("The child thread should return an error");
    }

    #[test]
    #[should_panic(expected = "This lock pool doesn't support poisoning")]
    fn test_unpoison_not_poisoned() {
        let p = TokioLockPool::new();
        let _ = p.unpoison(2);
    }

    #[test]
    #[should_panic(expected = "This lock pool doesn't support poisoning")]
    fn test_unpoison_poisoned() {
        let p = Arc::new(TokioLockPool::new());
        poison_lock(&p, 2);

        let _ = p.unpoison(2);
    }

    #[tokio::test]
    #[should_panic(
        expected = "Cannot block the current thread from within a runtime. This happens because a function attempted to block the current thread while the thread is being used to drive asynchronous tasks."
    )]
    async fn lock_from_async_context_with_sync_api() {
        let p = TokioLockPool::new();
        let _ = p.lock(3);
    }

    #[tokio::test]
    #[should_panic(
        expected = "Cannot block the current thread from within a runtime. This happens because a function attempted to block the current thread while the thread is being used to drive asynchronous tasks."
    )]
    async fn lock_owned_from_async_context_with_sync_api() {
        let p = Arc::new(TokioLockPool::new());
        let _ = p.lock_owned(3);
    }

    #[tokio::test]
    async fn test_simple_lock_unlock() {
        let pool = TokioLockPool::new();
        assert_eq!(0, pool.num_locked_or_poisoned());
        let guard = pool.lock_async(4).await;
        assert_eq!(1, pool.num_locked_or_poisoned());
        std::mem::drop(guard);
        assert_eq!(0, pool.num_locked_or_poisoned());
    }

    #[tokio::test]
    async fn test_simple_lock_owned_unlock() {
        let pool = Arc::new(TokioLockPool::new());
        assert_eq!(0, pool.num_locked_or_poisoned());
        let guard = pool.lock_owned_async(4).await;
        assert_eq!(1, pool.num_locked_or_poisoned());
        std::mem::drop(guard);
        assert_eq!(0, pool.num_locked_or_poisoned());
    }

    #[tokio::test]
    async fn test_multi_lock_unlock() {
        let pool = TokioLockPool::new();
        assert_eq!(0, pool.num_locked_or_poisoned());
        let guard1 = pool.lock_async(1).await;
        assert_eq!(1, pool.num_locked_or_poisoned());
        let guard2 = pool.lock_async(2).await;
        assert_eq!(2, pool.num_locked_or_poisoned());
        let guard3 = pool.lock_async(3).await;
        assert_eq!(3, pool.num_locked_or_poisoned());

        std::mem::drop(guard2);
        assert_eq!(2, pool.num_locked_or_poisoned());
        std::mem::drop(guard1);
        assert_eq!(1, pool.num_locked_or_poisoned());
        std::mem::drop(guard3);
        assert_eq!(0, pool.num_locked_or_poisoned());
    }

    #[tokio::test]
    async fn test_multi_lock_owned_unlock() {
        let pool = Arc::new(TokioLockPool::new());
        assert_eq!(0, pool.num_locked_or_poisoned());
        let guard1 = pool.lock_owned_async(1).await;
        assert_eq!(1, pool.num_locked_or_poisoned());
        let guard2 = pool.lock_owned_async(2).await;
        assert_eq!(2, pool.num_locked_or_poisoned());
        let guard3 = pool.lock_owned_async(3).await;
        assert_eq!(3, pool.num_locked_or_poisoned());

        std::mem::drop(guard2);
        assert_eq!(2, pool.num_locked_or_poisoned());
        std::mem::drop(guard1);
        assert_eq!(1, pool.num_locked_or_poisoned());
        std::mem::drop(guard3);
        assert_eq!(0, pool.num_locked_or_poisoned());
    }

    #[tokio::test]
    async fn test_concurrent_lock() {
        let pool = Arc::new(TokioLockPool::new());
        let guard = pool.lock_async(5).await;

        let counter = Arc::new(AtomicU32::new(0));

        let child = launch_locking_async_thread(&pool, 5, &counter, None);

        // Check that even if we wait, the child thread won't get the lock
        thread::sleep(Duration::from_millis(100));
        assert_eq!(0, counter.load(Ordering::SeqCst));

        // Check that we can stil lock other locks while the child is waiting
        {
            let _g = pool.lock_async(4).await;
        }

        // Now free the lock so the child can get it
        std::mem::drop(guard);

        // And check that the child got it
        child.join().unwrap();
        assert_eq!(1, counter.load(Ordering::SeqCst));

        assert_eq!(0, pool.num_locked_or_poisoned());
    }

    #[tokio::test]
    async fn test_concurrent_lock_owned() {
        let pool = Arc::new(TokioLockPool::new());
        let guard = pool.lock_owned_async(5).await;

        let counter = Arc::new(AtomicU32::new(0));

        let child = launch_locking_owned_async_thread(&pool, 5, &counter, None);

        // Check that even if we wait, the child thread won't get the lock
        thread::sleep(Duration::from_millis(100));
        assert_eq!(0, counter.load(Ordering::SeqCst));

        // Check that we can stil lock other locks while the child is waiting
        {
            let _g = pool.lock_owned_async(4).await;
        }

        // Now free the lock so the child can get it
        std::mem::drop(guard);

        // And check that the child got it
        child.join().unwrap();
        assert_eq!(1, counter.load(Ordering::SeqCst));

        assert_eq!(0, pool.num_locked_or_poisoned());
    }

    #[tokio::test]
    async fn test_multi_concurrent_lock() {
        let pool = Arc::new(TokioLockPool::new());
        let guard = pool.lock_async(5).await;

        let counter = Arc::new(AtomicU32::new(0));
        let barrier = Arc::new(tokio::sync::Mutex::new(()));
        let barrier_guard = barrier.lock().await;

        let child1 = launch_locking_async_thread(&pool, 5, &counter, Some(&barrier));
        let child2 = launch_locking_async_thread(&pool, 5, &counter, Some(&barrier));

        // Check that even if we wait, the child thread won't get the lock
        thread::sleep(Duration::from_millis(100));
        assert_eq!(0, counter.load(Ordering::SeqCst));

        // Check that we can stil lock other locks while the children are waiting
        {
            let _g = pool.lock_async(4).await;
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

    #[tokio::test]
    async fn test_multi_concurrent_lock_owned() {
        let pool = Arc::new(TokioLockPool::new());
        let guard = pool.lock_owned_async(5).await;

        let counter = Arc::new(AtomicU32::new(0));
        let barrier = Arc::new(tokio::sync::Mutex::new(()));
        let barrier_guard = barrier.lock().await;

        let child1 = launch_locking_owned_async_thread(&pool, 5, &counter, Some(&barrier));
        let child2 = launch_locking_owned_async_thread(&pool, 5, &counter, Some(&barrier));

        // Check that even if we wait, the child thread won't get the lock
        thread::sleep(Duration::from_millis(100));
        assert_eq!(0, counter.load(Ordering::SeqCst));

        // Check that we can stil lock other locks while the children are waiting
        {
            let _g = pool.lock_owned_async(4).await;
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

    #[tokio::test]
    async fn test_lock_owned_guards_can_be_passed_around() {
        let make_guard = || async {
            let pool = Arc::new(TokioLockPool::new());
            pool.lock_owned_async(5).await
        };
        let _guard = make_guard().await;
    }

    #[tokio::test]
    async fn test_lock_guards_can_be_held_across_await_points() {
        let task = async {
            let pool = TokioLockPool::new();
            let guard = pool.lock_async(3).await;
            tokio::time::sleep(Duration::from_millis(10)).await;
            std::mem::drop(guard);
        };

        // We also need to move the task to a different thread because
        // SyncLockPool **can** be used across an await but the task
        // isn't Send and cannot be moved to a differen thread.
        thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(task);
        });
    }

    #[tokio::test]
    async fn test_lock_owned_guards_can_be_held_across_await_points() {
        let task = async {
            let pool = Arc::new(TokioLockPool::new());
            let guard = pool.lock_owned_async(3).await;
            tokio::time::sleep(Duration::from_millis(10)).await;
            std::mem::drop(guard);
        };

        // We also need to move the task to a different thread because
        // SyncLockPool **can** be used across an await but the task
        // isn't Send and cannot be moved to a differen thread.
        thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(task);
        });
    }
}
