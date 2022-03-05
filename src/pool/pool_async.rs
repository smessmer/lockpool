use async_trait::async_trait;
use owning_ref::OwningHandle;
use std::fmt::Debug;
use std::hash::Hash;
use std::ops::Deref;
use std::sync::Arc;

use crate::pool::LockPool;
use crate::guard::Guard;

/// [AsyncLockPool] is an implementation of [LockPool] (see [LockPool] for API details) that can be used
/// in asynchronous code. It is a little slower than [SyncLockPool] but its locks can be held across
/// `await` points.
///
/// [AsyncLockPool] is based on top of [tokio::sync::Mutex] and does not support poisoning of locks.
/// See the [tokio::sync::Mutex] documentation for details on poisoning.
///
/// [AsyncLockPool] implements [LockPool] for when you want to lock in synchronous code. That API will
/// panic if called from asynchronous code, see the documentation of [tokio::sync::Mutex::blocking_lock].
/// For use in asynchronous code, [AsyncLockPool] also implements the [LockPoolAsync] API.
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
    crate::instantiate_common_tests!(common, crate::AsyncLockPool<isize>);
}
