use async_trait::async_trait;
use owning_ref::OwningHandle;
use std::fmt::Debug;
use std::hash::Hash;
use std::ops::Deref;
use std::sync::Arc;

use crate::guard::Guard;
use crate::pool::LockPool;

/// [AsyncLockPool] is an implementation of [LockPoolAsync] (see [LockPoolAsync] for API details) that can be used
/// in asynchronous code. It is a little slower than [SyncLockPool] but its locks can be held across
/// `await` points.
///
/// This implementation can also be used in synchronous code since it also implements the [LockPool] API,
/// but it will panic if you call [LockPool::lock] or [LockPool::lock_owned] from an `async` context,
/// see the documentation of [tokio::sync::Mutex::blocking_lock].
///
/// [AsyncLockPool] is based on top of [tokio::sync::Mutex] and does not support poisoning of locks.
/// See the [tokio::sync::Mutex] documentation for details on poisoning.
#[cfg(feature = "tokio")]
pub type AsyncLockPool<K> = super::LockPoolImpl<K, tokio::sync::Mutex<()>>;

/// TODO
#[async_trait]
pub trait LockPoolAsync<K>: LockPool<K>
where
    K: Eq + PartialEq + Hash + Clone + Debug + Send,
{
    /// TODO
    async fn lock_async(&self, key: K) -> Self::Guard<'_>;

    /// TODO
    async fn lock_owned_async<'a>(self: &'a Arc<Self>, key: K) -> Self::OwnedGuard;
}

#[async_trait]
impl<K> LockPoolAsync<K> for AsyncLockPool<K>
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

impl<K> AsyncLockPool<K>
where
    K: Eq + PartialEq + Hash + Clone + Debug + Send + 'static,
{
    async fn _lock_async<'a, S: 'a + Deref<Target = Self>>(
        this: S,
        key: K,
    ) -> Guard<'a, K, tokio::sync::Mutex<()>, S> {
        let mutex = this._load_or_insert_mutex_for_key(&key);
        // Now we have an Arc::clone of the mutex for this key, and the global mutex is already unlocked so other threads can access the hash map.
        // The following blocks until the mutex for this key is acquired.

        let guard =
            OwningHandle::new_with_async_fn(mutex, |mutex: *const tokio::sync::Mutex<()>| {
                let mutex: &tokio::sync::Mutex<()> = unsafe { &*mutex };
                mutex.lock()
            })
            .await;
        Guard::new(this, key, guard, false)
    }
}

#[cfg(test)]
mod tests {
    use super::{LockPoolAsync, AsyncLockPool};
    use crate::LockPool;
    use std::sync::Arc;
    use std::thread;

    crate::instantiate_common_tests!(common, super::AsyncLockPool<isize>);

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
        let p = AsyncLockPool::new();
        let _ = p.unpoison(2);
    }

    #[test]
    #[should_panic(expected = "This lock pool doesn't support poisoning")]
    fn test_unpoison_poisoned() {
        let p = Arc::new(AsyncLockPool::new());
        poison_lock(&p, 2);

        let _ = p.unpoison(2);
    }

    #[tokio::test]
    #[should_panic(expected = "Cannot start a runtime from within a runtime. This happens because a function (like `block_on`) attempted to block the current thread while the thread is being used to drive asynchronous tasks.")]
    async fn lock_from_async_context_with_sync_api() {
        let p = AsyncLockPool::new();
        let _ = p.lock(3);
    }

    #[tokio::test]
    #[should_panic(expected = "Cannot start a runtime from within a runtime. This happens because a function (like `block_on`) attempted to block the current thread while the thread is being used to drive asynchronous tasks.")]
    async fn lock_owned_from_async_context_with_sync_api() {
        let p = Arc::new(AsyncLockPool::new());
        let _ = p.lock_owned(3);
    }

    // TODO Test LockPoolAsync API
}
