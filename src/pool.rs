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
    /// This function might panic when called if the lock is already held by the current thread.
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

/// This is a pool of locks where individual locks can be locked/unlocked by key. It initially considers all keys as "unlocked", but they can be locked
/// and if a second thread tries to acquire a lock for the same key, they will have to wait.
///
/// Under the hood, a [LockPool] is a [HashMap] of [Mutex]es, with some logic making sure there aren't any race conditions when accessing the hash map.
///
/// Example:
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
///
pub struct LockPoolImpl<K, M>
where
    K: Eq + PartialEq + Hash + Clone + Debug,
    M: MutexImpl,
{
    currently_locked: Mutex<HashMap<K, Arc<M>>>,
    _p: PhantomData<M>,
}

impl<K, M> Default for LockPoolImpl<K, M>
where
    K: Eq + PartialEq + Hash + Clone + Debug,
    M: MutexImpl,
{
    fn default() -> Self {
        Self {
            currently_locked: Mutex::new(HashMap::new()),
            _p: PhantomData,
        }
    }
}

impl<K, M> LockPool<K> for LockPoolImpl<K, M>
where
    K: Eq + PartialEq + Hash + Clone + Debug + 'static,
    M: MutexImpl + 'static,
{
    type Guard<'a> = Guard<'a, K, M, &'a Self>;
    type OwnedGuard = Guard<'static, K, M, Arc<LockPoolImpl<K, M>>>;

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

    fn _load_or_insert_mutex_for_key(&self, key: &K) -> Arc<M> {
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

/// [SyncLockPool] is an implementation of [LockPool] (see [LockPool] for API details) that can be used
/// in synchronous code. It is a little faster than [AsyncLockPool] but its locks cannot be held across
/// `await` points.
///
/// [SyncLockPool] is based on top of [std::sync::Mutex] and supports poisoning of locks.
/// See the [std::sync::Mutex] documentation for details on poisoning.
pub type SyncLockPool<K> = LockPoolImpl<K, std::sync::Mutex<()>>;

/// [AsyncLockPool] is an implementation of [LockPool] (see [LockPool] for API details) that can be used
/// in asynchronous code. It is a little slower than [SyncLockPool] but its locks can be held across
/// `await` points.
///
/// [AsyncLockPool] is based on top of [tokio::sync::Mutex] and does not support poisoning of locks.
/// See the [tokio::sync::Mutex] documentation for details on poisoning.
///
/// [AsyncLockPool] implements [LockPool] for when you want to lock in synchronous code. That API will
/// panic if called from asynchronous code, see the documentation of [tokio::sync::Mutex::blocking_lock].
/// For use in asynchronous code, [AsyncLockPool] also implements toe [TODO] API.
#[cfg(feature = "tokio")]
pub type AsyncLockPool<K> = LockPoolImpl<K, tokio::sync::Mutex<()>>;

#[cfg(test)]
mod tests {
    #[cfg(feature = "tokio")]
    use super::AsyncLockPool;
    use super::{LockPool, SyncLockPool};
    use crate::{PoisonError, TryLockError, UnpoisonError};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    // TODO Exclude poisoning tests from AsyncLockPool since it doesn't have poisoning

    fn test_simple_lock_unlock<P: LockPool<isize>>() {
        let pool = P::new();
        assert_eq!(0, pool.num_locked_or_poisoned());
        let guard = pool.lock(4).unwrap();
        assert_eq!(1, pool.num_locked_or_poisoned());
        std::mem::drop(guard);
        assert_eq!(0, pool.num_locked_or_poisoned());
    }

    fn test_simple_lock_owned_unlock<P: LockPool<isize>>() {
        let pool = Arc::new(P::new());
        assert_eq!(0, pool.num_locked_or_poisoned());
        let guard = pool.lock_owned(4).unwrap();
        assert_eq!(1, pool.num_locked_or_poisoned());
        std::mem::drop(guard);
        assert_eq!(0, pool.num_locked_or_poisoned());
    }

    fn test_simple_try_lock_unlock<P: LockPool<isize>>() {
        let pool = P::new();
        assert_eq!(0, pool.num_locked_or_poisoned());
        let guard = pool.try_lock(4).unwrap();
        assert_eq!(1, pool.num_locked_or_poisoned());
        std::mem::drop(guard);
        assert_eq!(0, pool.num_locked_or_poisoned());
    }

    fn test_simple_try_lock_owned_unlock<P: LockPool<isize>>() {
        let pool = Arc::new(P::new());
        assert_eq!(0, pool.num_locked_or_poisoned());
        let guard = pool.try_lock_owned(4).unwrap();
        assert_eq!(1, pool.num_locked_or_poisoned());
        std::mem::drop(guard);
        assert_eq!(0, pool.num_locked_or_poisoned());
    }

    fn test_multi_lock_unlock<P: LockPool<isize>>() {
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

    fn test_multi_lock_owned_unlock<P: LockPool<isize>>() {
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

    fn test_multi_try_lock_unlock<P: LockPool<isize>>() {
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

    fn test_multi_try_lock_owned_unlock<P: LockPool<isize>>() {
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

    // Launch a thread that
    // 1. locks the given key
    // 2. once it has the lock, increments a counter
    // 3. then waits until a barrier is released before it releases the lock
    fn launch_locking_thread<P: LockPool<isize> + Send + Sync + 'static>(
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

    fn launch_locking_owned_thread<P: LockPool<isize> + Send + Sync + 'static>(
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

    fn test_concurrent_lock<P: LockPool<isize> + Send + Sync + 'static>() {
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

    fn test_concurrent_lock_owned<P: LockPool<isize> + Send + Sync + 'static>() {
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

    fn test_concurrent_try_lock<P: LockPool<isize>>() {
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

    fn test_concurrent_try_lock_owned<P: LockPool<isize>>() {
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

    fn test_multi_concurrent_lock<P: LockPool<isize> + Send + Sync + 'static>() {
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

    fn test_multi_concurrent_lock_owned<P: LockPool<isize> + Send + Sync + 'static>() {
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

    fn test_pool_mutex_poisoned_by_lock<P: LockPool<isize> + Send + Sync + 'static>() {
        let pool = Arc::new(P::new());

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

    fn test_pool_mutex_poisoned_by_lock_owned<P: LockPool<isize> + Send + Sync + 'static>() {
        let pool = Arc::new(P::new());

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

    fn test_pool_mutex_poisoned_by_try_lock<P: LockPool<isize> + Send + Sync + 'static>() {
        let pool = Arc::new(P::new());

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

    fn test_pool_mutex_poisoned_by_try_lock_owned<P: LockPool<isize> + Send + Sync + 'static>() {
        let pool = Arc::new(P::new());

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

    fn test_poison_error_holds_lock<P: LockPool<isize> + Send + Sync + 'static>() {
        let pool = Arc::new(P::new());
        poison_lock(&pool, 5);
        let error = pool.lock(5).unwrap_err();
        assert_is_lock_poisoned_error(5, &error);

        let counter = Arc::new(AtomicU32::new(0));

        let child = launch_locking_thread(&pool, 5, &counter, None);

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

    fn test_poison_error_holds_lock_owned<P: LockPool<isize> + Send + Sync + 'static>() {
        let pool = Arc::new(P::new());
        poison_lock_owned(&pool, 5);
        let error = pool.lock_owned(5).unwrap_err();
        assert_is_lock_poisoned_error(5, &error);

        let counter = Arc::new(AtomicU32::new(0));

        let child = launch_locking_owned_thread(&pool, 5, &counter, None);

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

    fn test_poison_error_holds_try_lock<P: LockPool<isize> + Send + Sync + 'static>() {
        let pool = Arc::new(P::new());
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

    fn test_poison_error_holds_try_lock_owned<P: LockPool<isize> + Send + Sync + 'static>() {
        let pool = Arc::new(P::new());
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

    fn test_unpoison<P: LockPool<isize> + Send + Sync + 'static>() {
        let pool = Arc::new(P::new());

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

    fn test_unpoison_not_poisoned<P: LockPool<isize>>() {
        let pool = Arc::new(P::new());

        let err = pool.unpoison(3).unwrap_err();
        assert_eq!(UnpoisonError::NotPoisoned, err);
    }

    fn test_unpoison_while_other_thread_waiting<P: LockPool<isize> + Send + Sync + 'static>() {
        let pool = Arc::new(P::new());

        poison_lock(&pool, 3);

        let _err_guard = pool.lock(3).unwrap_err();

        // _err_guard keeps it locked

        let err = pool.unpoison(3).unwrap_err();
        assert!(matches!(err, UnpoisonError::OtherThreadsBlockedOnMutex));
    }

    macro_rules! instantiate_test {
        ($lock_pool:ty, $test_name:ident) => {
            #[test]
            fn $test_name() {
                super::$test_name::<$lock_pool>();
            }
        };
    }

    macro_rules! instantiate_tests {
        ($type_name: ident, $lock_pool:ty) => {
            mod $type_name {
                // TODO There's still lots of duplication between the normal and the _owned tests.
                //      Can we deduplicate this similar to how we deduplicated AsyncLockPool vs SyncLockPool tests with a macro here?
                instantiate_test!($lock_pool, test_simple_lock_unlock);
                instantiate_test!($lock_pool, test_simple_lock_owned_unlock);
                instantiate_test!($lock_pool, test_simple_try_lock_unlock);
                instantiate_test!($lock_pool, test_simple_try_lock_owned_unlock);
                instantiate_test!($lock_pool, test_multi_lock_unlock);
                instantiate_test!($lock_pool, test_multi_lock_owned_unlock);
                instantiate_test!($lock_pool, test_multi_try_lock_unlock);
                instantiate_test!($lock_pool, test_multi_try_lock_owned_unlock);
                instantiate_test!($lock_pool, test_concurrent_lock);
                instantiate_test!($lock_pool, test_concurrent_lock_owned);
                instantiate_test!($lock_pool, test_concurrent_try_lock);
                instantiate_test!($lock_pool, test_concurrent_try_lock_owned);
                instantiate_test!($lock_pool, test_multi_concurrent_lock);
                instantiate_test!($lock_pool, test_multi_concurrent_lock_owned);
                instantiate_test!($lock_pool, test_pool_mutex_poisoned_by_lock);
                instantiate_test!($lock_pool, test_pool_mutex_poisoned_by_lock_owned);
                instantiate_test!($lock_pool, test_pool_mutex_poisoned_by_try_lock);
                instantiate_test!($lock_pool, test_pool_mutex_poisoned_by_try_lock_owned);
                instantiate_test!($lock_pool, test_poison_error_holds_lock);
                instantiate_test!($lock_pool, test_poison_error_holds_lock_owned);
                instantiate_test!($lock_pool, test_poison_error_holds_try_lock);
                instantiate_test!($lock_pool, test_poison_error_holds_try_lock_owned);
                instantiate_test!($lock_pool, test_unpoison);
                instantiate_test!($lock_pool, test_unpoison_not_poisoned);
                instantiate_test!($lock_pool, test_unpoison_while_other_thread_waiting);
            }
        };
    }

    instantiate_tests!(sync_lock_pool, super::SyncLockPool<isize>);
    #[cfg(feature = "tokio")]
    instantiate_tests!(async_lock_pool, super::AsyncLockPool<isize>);
}
