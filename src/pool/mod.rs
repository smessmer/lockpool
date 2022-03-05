use owning_ref::OwningHandle;
use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;
use std::marker::PhantomData;
use std::ops::Deref;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::error::{PoisonError, TryLockError, UnpoisonError};
use crate::guard::Guard;
use crate::mutex::{LockError, MutexImpl};

/// A pool of locks where individual locks can be locked/unlocked by key.
/// It initially considers all keys as "unlocked", but they can be locked
/// and if a second thread tries to acquire a lock for the same key, they will have to wait.
///
/// This trait is implemented by [AsyncLockPool] and [SyncLockPool]. [SyncLockPool] is a little faster
/// but its locks cannot be held across `await` points in asynchronous code. [AsyncLockPool] can
/// be used in both synchronous and asynchronous code and offers methods for each.
///
/// ```
/// use lockpool::{LockPool, SyncLockPool};
///
/// let pool = SyncLockPool::new();
/// # (|| -> Result<(), lockpool::PoisonError<_, _>> {
/// let guard1 = pool.lock(4)?;
/// let guard2 = pool.lock(5)?;
///
/// // This next line would cause a deadlock or panic because `4` is already locked on this thread
/// // let guard3 = pool.lock(4)?;
///
/// // After dropping the corresponding guard, we can lock it again
/// std::mem::drop(guard1);
/// let guard3 = pool.lock(4)?;
/// # Ok(())
/// # })().unwrap();
/// ```
///
/// You can use an arbitrary type to index locks by, as long as that type implements [PartialEq] + [Eq] + [Hash] + [Clone] + [Debug].
///
/// ```
/// use lockpool::{LockPool, SyncLockPool};
///
/// #[derive(PartialEq, Eq, Hash, Clone, Debug)]
/// struct CustomLockKey(u32);
///
/// let pool = SyncLockPool::new();
/// # (|| -> Result<(), lockpool::PoisonError<_, _>> {
/// let guard = pool.lock(CustomLockKey(4))?;
/// # Ok(())
/// # })().unwrap();
/// ```
///
/// Under the hood, a [LockPool] is a [HashMap](std::collections::HashMap) of [Mutex](std::sync::Mutex)es, with some logic making sure there aren't any race conditions when accessing the hash map.
pub trait LockPool<K>: Default
where
    K: Eq + PartialEq + Hash + Clone + Debug,
{
    /// A handle to a held lock. The guard cannot be held across .await points.
    /// The guard internally borrows the [LockPool], so the [LockPool] will not be dropped while a guard exists.
    /// The lock is automatically released whenever the guard is dropped, at which point [LockPool::lock] or [LockPool::lock_owned] with the same key will succeed yet again.
    type Guard<'a>: Debug
    where
        Self: 'a;

    /// An owned handle to a held lock.
    /// This guard is only available from a [LockPool] that is wrapped in an [Arc]. It is identical to [LockPool::Guard], except that rather than borrowing the [LockPool], it clones the [Arc], incrementing the reference count.
    /// This means that unlike [LockPool::Guard], it will have the `'static` lifetime.
    /// The lock is automatically released whenever the guard is dropped, at which point [LockPool::lock] or [LockPool::lock_owned] with the same key will succeed yet again.
    type OwnedGuard: Debug;

    /// Create a new lock pool where no lock is locked
    #[inline]
    fn new() -> Self {
        Self::default()
    }

    /// Return the number of locked locks
    ///
    /// Corner case: Poisoned locks count as locked even if they're currently not locked
    fn num_locked_or_poisoned(&self) -> usize;

    /// Lock a lock by key.
    ///
    /// If the lock with this key is currently locked by a different thread, then the current thread blocks until it becomes available.
    /// Upon returning, the thread is the only thread with the lock held. A RAII guard is returned to allow scoped unlock
    /// of the lock. When the guard goes out of scope, the lock will be unlocked.
    ///
    /// The exact behavior on locking a lock in the thread which already holds the lock is left unspecified.
    /// However, this function will not return on the second call (it might panic or deadlock, for example).
    ///
    /// Errors
    /// -----
    /// If another user of this lock panicked while holding the lock, then this call will return an error once the lock is acquired.
    ///
    /// Panics
    /// -----
    /// - This function might panic when called if the lock is already held by the current thread.
    /// - If this is called through [AsyncLockPool], then this function will also panic when called from an `async` context.
    ///   See documentation of [AsyncLockPool] for details.
    ///
    /// Examples
    /// -----
    /// ```
    /// use lockpool::{LockPool, SyncLockPool};
    ///
    /// let pool = SyncLockPool::new();
    /// # (|| -> Result<(), lockpool::PoisonError<_, _>> {
    /// let guard1 = pool.lock(4)?;
    /// let guard2 = pool.lock(5)?;
    ///
    /// // This next line would cause a deadlock or panic because `4` is already locked on this thread
    /// // let guard3 = pool.lock(4)?;
    ///
    /// // After dropping the corresponding guard, we can lock it again
    /// std::mem::drop(guard1);
    /// let guard3 = pool.lock(4)?;
    /// # Ok(())
    /// # })().unwrap();
    /// ```
    fn lock(&self, key: K) -> Result<Self::Guard<'_>, PoisonError<K, Self::Guard<'_>>>;

    /// Lock a lock by key.
    ///
    /// This is similar to [LockPool::lock], but it works on an `Arc<LockPool>` instead of a [LockPool] and
    /// returns a [Guard] that binds its lifetime to the [LockPool] in that [Arc]. Such a [Guard] can be more
    /// easily moved around or cloned.
    ///
    /// Errors
    /// -----
    /// If another user of this lock panicked while holding the lock, then this call will return an error once the lock is acquired.
    ///
    /// Panics
    /// -----
    /// - This function might panic when called if the lock is already held by the current thread.
    /// - If this is called through [AsyncLockPool], then this function will also panic when called from an `async` context.
    ///   See documentation of [AsyncLockPool] for details.
    ///
    /// Examples
    /// -----
    /// ```
    /// use lockpool::{LockPool, SyncLockPool};
    /// use std::sync::Arc;
    ///
    /// let pool = Arc::new(SyncLockPool::new());
    /// # (|| -> Result<(), lockpool::PoisonError<_, _>> {
    /// let guard1 = pool.lock_owned(4)?;
    /// let guard2 = pool.lock_owned(5)?;
    ///
    /// // This next line would cause a deadlock or panic because `4` is already locked on this thread
    /// // let guard3 = pool.lock_owned(4)?;
    ///
    /// // After dropping the corresponding guard, we can lock it again
    /// std::mem::drop(guard1);
    /// let guard3 = pool.lock_owned(4)?;
    /// # Ok(())
    /// # })().unwrap();
    /// ```
    fn lock_owned(
        self: &Arc<Self>,
        key: K,
    ) -> Result<Self::OwnedGuard, PoisonError<K, Self::OwnedGuard>>;

    /// Attempts to acquire the lock with the given key.
    ///
    /// If the lock could not be acquired at this time, then [Err] is returned. Otherwise, a RAII guard is returned.
    /// The lock will be unlocked when the guard is dropped.
    ///
    /// This function does not block.
    ///
    /// Errors
    /// -----
    /// - If another user of this lock panicked while holding the lock, then this call will return [TryLockError::Poisoned].
    /// - If the lock could not be acquired because it is already locked, then this call will return [TryLockError::WouldBlock].
    ///
    /// Examples
    /// -----
    /// ```
    /// use lockpool::{TryLockError, LockPool, SyncLockPool};
    ///
    /// let pool = SyncLockPool::new();
    /// # (|| -> Result<(), lockpool::PoisonError<_, _>> {
    /// let guard1 = pool.lock(4)?;
    /// let guard2 = pool.lock(5)?;
    ///
    /// // This next line would cause a deadlock or panic because `4` is already locked on this thread
    /// let guard3 = pool.try_lock(4);
    /// assert!(matches!(guard3.unwrap_err(), TryLockError::WouldBlock));
    ///
    /// // After dropping the corresponding guard, we can lock it again
    /// std::mem::drop(guard1);
    /// let guard3 = pool.lock(4)?;
    /// # Ok(())
    /// # })().unwrap();
    /// ```
    fn try_lock(&self, key: K) -> Result<Self::Guard<'_>, TryLockError<K, Self::Guard<'_>>>;

    /// Attempts to acquire the lock with the given key.
    ///
    /// This is similar to [LockPool::try_lock], but it works on an `Arc<LockPool>` instead of a [LockPool] and
    /// returns a [Guard] that binds its lifetime to the [LockPool] in that [Arc]. Such a [Guard] can be more
    /// easily moved around or cloned.
    ///
    /// This function does not block.
    ///
    /// Errors
    /// -----
    /// - If another user of this lock panicked while holding the lock, then this call will return [TryLockError::Poisoned].
    /// - If the lock could not be acquired because it is already locked, then this call will return [TryLockError::WouldBlock].
    ///
    /// Examples
    /// -----
    /// ```
    /// use lockpool::{TryLockError, LockPool, SyncLockPool};
    /// use std::sync::Arc;
    ///
    /// let pool = Arc::new(SyncLockPool::new());
    /// # (|| -> Result<(), lockpool::PoisonError<_, _>> {
    /// let guard1 = pool.lock(4)?;
    /// let guard2 = pool.lock(5)?;
    ///
    /// // This next line would cause a deadlock or panic because `4` is already locked on this thread
    /// let guard3 = pool.try_lock_owned(4);
    /// assert!(matches!(guard3.unwrap_err(), TryLockError::WouldBlock));
    ///
    /// // After dropping the corresponding guard, we can lock it again
    /// std::mem::drop(guard1);
    /// let guard3 = pool.lock(4)?;
    /// # Ok(())
    /// # })().unwrap();
    /// ```
    fn try_lock_owned(
        self: &Arc<Self>,
        key: K,
    ) -> Result<Self::OwnedGuard, TryLockError<K, Self::OwnedGuard>>;

    /// Unpoisons a poisoned lock.
    ///
    /// Generally, once a thread panics while a lock is held, that lock is poisoned forever and all future attempts at locking it will return a [PoisonError].
    /// This is since the resources protected by the lock are likely in an invalid state if the thread panicked while having the lock.
    ///
    /// However, if you need an escape hatch, this function is it. Using it, you can unpoison a lock so that it can be locked again.
    /// This only works if currently no other thread is waiting for the lock.
    ///
    /// Errors:
    /// -----
    /// - Returns [UnpoisonError::NotPoisoned] if the lock wasn't actually poisoned
    /// - Returns [UnpoisonError::OtherThreadsBlockedOnMutex] if there are other threads currently waiting for this lock
    fn unpoison(&self, key: K) -> Result<(), UnpoisonError>;
}

/// This struct implements both [SyncLockPool] and [AsyncLockPool]. See [LockPool] for the API.
pub struct LockPoolImpl<K, M>
where
    K: Eq + PartialEq + Hash + Clone + Debug,
    M: MutexImpl,
{
    // We always use std::sync::Mutex for protecting the HashMap since its guards
    // never have to be kept across await boundaries, but the inner per-key locks
    // can be different types of mutexes, determined by the `M` type parameter.
    currently_locked: Mutex<HashMap<K, Arc<M>>>,
    _p: PhantomData<M>,
}

impl<K, M> Default for LockPoolImpl<K, M>
where
    K: Eq + PartialEq + Hash + Clone + Debug,
    M: MutexImpl,
{
    #[inline]
    fn default() -> Self {
        Self {
            currently_locked: Mutex::new(HashMap::new()),
            _p: PhantomData,
        }
    }
}

impl<K, M> LockPool<K> for LockPoolImpl<K, M>
where
    // TODO Can we remove the 'static bound from K?
    K: Eq + PartialEq + Hash + Clone + Debug + 'static,
    M: MutexImpl + 'static,
{
    type Guard<'a> = Guard<'a, K, M, &'a Self>;
    type OwnedGuard = Guard<'static, K, M, Arc<LockPoolImpl<K, M>>>;

    #[inline]
    fn num_locked_or_poisoned(&self) -> usize {
        self._currently_locked().len()
    }

    fn lock(&self, key: K) -> Result<Self::Guard<'_>, PoisonError<K, Self::Guard<'_>>> {
        Self::_lock(self, key)
    }

    fn lock_owned(
        self: &Arc<Self>,
        key: K,
    ) -> Result<Self::OwnedGuard, PoisonError<K, Self::OwnedGuard>> {
        Self::_lock(Arc::clone(self), key)
    }

    fn try_lock(&self, key: K) -> Result<Self::Guard<'_>, TryLockError<K, Self::Guard<'_>>> {
        Self::_try_lock(self, key)
    }

    fn try_lock_owned(
        self: &Arc<Self>,
        key: K,
    ) -> Result<Self::OwnedGuard, TryLockError<K, Self::OwnedGuard>> {
        Self::_try_lock(Arc::clone(self), key)
    }

    fn unpoison(&self, key: K) -> Result<(), UnpoisonError> {
        if !M::SUPPORTS_POISONING {
            panic!("This lock pool doesn't support poisoning");
        }

        let mut currently_locked = self._currently_locked();
        // TODO Alternative idea: Keep currently_locked locked so that no new threads can request locks, then wait until all
        // current threads have gotten and released their locks, then unpoison. This should get rid of the OtherThreadsBlockedOnMutex error case.
        let mutex: &Arc<M> = currently_locked
            .get(&key)
            .ok_or(UnpoisonError::NotPoisoned)?;
        if Arc::strong_count(mutex) != 1 {
            return Err(UnpoisonError::OtherThreadsBlockedOnMutex);
        }
        let result = match Arc::clone(mutex).lock() {
            Ok(_) => Err(UnpoisonError::NotPoisoned),
            Err(_) => {
                // If we're here, then we know that the mutex is in fact poisoned and there's no other threads having access to the mutex at the moment.
                // Thanks to the global lock on currently_locked, we know that no other thread can get access to it either at the moment.
                // The best way to unpoison the mutex is to just remove it. It will be recreated the next time somebody asks for it.
                let remove_result = currently_locked.remove(&key);
                assert!(
                    remove_result.is_some(),
                    "We just got this entry above from the hash map, it cannot have vanished since then"
                );
                Ok(())
            }
        };
        result
    }
}

impl<K, M> LockPoolImpl<K, M>
where
    K: Eq + PartialEq + Hash + Clone + Debug,
    M: MutexImpl,
{
    fn _currently_locked(&self) -> MutexGuard<'_, HashMap<K, Arc<M>>> {
        self.currently_locked
            .lock()
            .expect("The global mutex protecting the lock pool is poisoned. This shouldn't happen since there shouldn't be any user code running while this lock is held so no thread should ever panic with it")
    }

    pub(super) fn _load_or_insert_mutex_for_key(&self, key: &K) -> Arc<M> {
        let mut currently_locked = self._currently_locked();
        if let Some(mutex) = currently_locked.get_mut(key).map(|a| Arc::clone(a)) {
            mutex
        } else {
            let insert_result = currently_locked.insert(key.clone(), Arc::new(M::new()));
            assert!(
                insert_result.is_none(),
                "We just checked that the entry doesn't exist, why does it exist now?"
            );
            currently_locked
                .get_mut(key)
                .map(|a| Arc::clone(a))
                .expect("We just inserted this")
        }
    }

    fn _lock<'a, S: 'a + Deref<Target = Self>>(
        this: S,
        key: K,
    ) -> Result<Guard<'a, K, M, S>, PoisonError<K, Guard<'a, K, M, S>>> {
        let mutex = this._load_or_insert_mutex_for_key(&key);
        // Now we have an Arc::clone of the mutex for this key, and the global mutex is already unlocked so other threads can access the hash map.
        // The following blocks until the mutex for this key is acquired.

        let mut poisoned = false;
        let guard = OwningHandle::new_with_fn(mutex, |mutex: *const M| {
            let mutex: &M = unsafe { &*mutex };
            match mutex.lock() {
                Ok(guard) => guard,
                Err(poison_error) => {
                    poisoned = true;
                    poison_error.into_inner()
                }
            }
        });
        if poisoned {
            let guard = Guard::new(this, key.clone(), guard, true);
            Err(PoisonError { key, guard })
        } else {
            let guard = Guard::new(this, key, guard, false);
            Ok(guard)
        }
    }

    fn _try_lock<'a, S: 'a + Deref<Target = Self>>(
        this: S,
        key: K,
    ) -> Result<Guard<'a, K, M, S>, TryLockError<K, Guard<'a, K, M, S>>> {
        let mutex = this._load_or_insert_mutex_for_key(&key);
        // Now we have an Arc::clone of the mutex for this key, and the global mutex is already unlocked so other threads can access the hash map.
        // The following tries to lock the mutex.

        let mut poisoned = false;
        let guard = OwningHandle::try_new(mutex, |mutex: *const M| {
            let mutex: &M = unsafe { &*mutex };
            match mutex.try_lock() {
                Ok(guard) => Ok(guard),
                Err(std::sync::TryLockError::Poisoned(poison_error)) => {
                    poisoned = true;
                    Ok(poison_error.into_inner())
                }
                Err(std::sync::TryLockError::WouldBlock) => Err(TryLockError::WouldBlock),
            }
        })?;
        if poisoned {
            let guard = Guard::new(this, key.clone(), guard, true);
            Err(TryLockError::Poisoned(PoisonError { key, guard }))
        } else {
            let guard = Guard::new(this, key, guard, false);
            Ok(guard)
        }
    }

    pub(super) fn _unlock(
        &self,
        key: &K,
        guard: OwningHandle<Arc<M>, M::Guard<'_>>,
        poisoned: bool,
    ) {
        if poisoned {
            // If the guard is poisoned, then we still unlock the lock but this happens by dropping
            // the guard. We keep poisoned locks in the hashmap.
            return;
        }

        let mut currently_locked = self._currently_locked();
        let mutex: &Arc<M> = currently_locked
            .get(key)
            .expect("This entry must exist or the guard passed in as a parameter shouldn't exist");
        std::mem::drop(guard);

        // Now the guard is dropped and the lock for this key is unlocked.
        // If there are any other Self::lock() calls for this key already running and
        // waiting for the mutex, they will be unblocked now and their guard
        // will be created.
        // But since we still have the global mutex on self.currently_locked, currently no
        // thread can newly call Self::lock() and create a clone of our Arc. Similarly,
        // no other thread can enter Self::unlock() and reduce the strong_count of the Arc.
        // This means that if Arc::strong_count() == 1, we know that we can clean up
        // without race conditions.

        if Arc::strong_count(mutex) == 1 && !std::thread::panicking() {
            // The guard we're about to drop is the last guard for this mutex,
            // the only other Arc pointing to it is the one in the hashmap.
            // We can clean up
            // We don't clean up if we're panicking because we want to keep the
            // lock poisoned in this case.
            let remove_result = currently_locked.remove(key);
            assert!(
                remove_result.is_some(),
                "We just got this entry above from the hash map, it cannot have vanished since then"
            );
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(feature = "tokio")]
pub mod pool_async;
pub mod pool_sync;
