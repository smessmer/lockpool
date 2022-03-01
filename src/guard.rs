use owning_ref::OwningHandle;
use std::fmt::{self, Debug};
use std::hash::Hash;
use std::ops::Deref;
use std::sync::Arc;

use super::mutex::MutexImpl;
use crate::pool::LockPoolImpl;

/// A RAII implementation of a scoped lock for locks from a [LockPool]. When this instance is dropped (falls out of scope), the lock will be unlocked.
#[must_use = "if unused the Mutex will immediately unlock"]
pub struct Guard<'a, K, M, P>
where
    K: Eq + PartialEq + Hash + Clone + Debug,
    M: MutexImpl + 'a,
    P: Deref<Target = LockPoolImpl<K, M>>,
{
    pool: P,
    key: K,
    guard: Option<OwningHandle<Arc<M>, M::Guard<'a>>>,
    poisoned: bool,
}

impl<'a, K, M, P> Guard<'a, K, M, P>
where
    K: Eq + PartialEq + Hash + Clone + Debug,
    M: MutexImpl + 'a,
    P: Deref<Target = LockPoolImpl<K, M>>,
{
    pub(super) fn new(
        pool: P,
        key: K,
        guard: OwningHandle<Arc<M>, M::Guard<'a>>,
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

impl<'a, K, M, P> Drop for Guard<'a, K, M, P>
where
    K: Eq + PartialEq + Hash + Clone + Debug,
    M: MutexImpl + 'a,
    P: Deref<Target = LockPoolImpl<K, M>>,
{
    fn drop(&mut self) {
        let guard = self
            .guard
            .take()
            .expect("The self.guard field must always be set unless this was already destructed");
        self.pool._unlock(&self.key, guard, self.poisoned);
    }
}

impl<'a, K, M, P> Debug for Guard<'a, K, M, P>
where
    K: Eq + PartialEq + Hash + Clone + Debug,
    M: MutexImpl + 'a,
    P: Deref<Target = LockPoolImpl<K, M>>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Guard({:?})", self.key)
    }
}
