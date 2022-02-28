use owning_ref::OwningHandle;
use std::fmt::{self, Debug};
use std::hash::Hash;
use std::sync::{Arc, Mutex, MutexGuard};
use std::ops::Deref;

use super::LockPool;

/// A RAII implementation of a scoped lock for locks from a [LockPool]. When this structure is dropped (falls out of scope), the lock will be unlocked.
#[must_use = "if unused the Mutex will immediately unlock"]
pub struct Guard<K, P>
where
    K: Eq + PartialEq + Hash + Clone + Debug,
    P: Deref<Target=LockPool<K>>,
{
    pool: P,
    key: K,
    guard: Option<OwningHandle<Arc<Mutex<()>>, MutexGuard<'static, ()>>>,
    poisoned: bool,
}

impl<K, P> Guard<K, P>
where
    K: Eq + PartialEq + Hash + Clone + Debug,
    P: Deref<Target=LockPool<K>>,
{
    pub(super) fn new(
        pool: P,
        key: K,
        guard: OwningHandle<Arc<Mutex<()>>, MutexGuard<'static, ()>>,
        poisoned: bool,
    ) -> Self {
        Self {
            pool,
            key,
            guard: Some(guard),
            poisoned,
        }
    }
}

impl<K, P> Drop for Guard<K, P>
where
    K: Eq + PartialEq + Hash + Clone + Debug,
    P: Deref<Target=LockPool<K>>,
{
    fn drop(&mut self) {
        let guard = self
            .guard
            .take()
            .expect("The self.guard field must always be set unless this was already destructed");
        self.pool._unlock(&self.key, guard, self.poisoned);
    }
}

impl<K, P> Debug for Guard<K, P>
where
    K: Eq + PartialEq + Hash + Clone + Debug,
    P: Deref<Target=LockPool<K>>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Guard({:?})", self.key)
    }
}
