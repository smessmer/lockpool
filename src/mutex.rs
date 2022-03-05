pub trait LockError<'a> {
    type Guard;
    fn into_inner(self) -> Self::Guard;
}

impl<'a, T> LockError<'a> for std::sync::PoisonError<std::sync::MutexGuard<'a, T>> {
    type Guard = std::sync::MutexGuard<'a, T>;
    fn into_inner(self) -> Self::Guard {
        std::sync::PoisonError::into_inner(self)
    }
}

#[cfg(feature = "tokio")]
pub struct Never<T> {
    _p: std::marker::PhantomData<T>,
    _n: !,
}

#[cfg(feature = "tokio")]
impl<'a, T: 'a> LockError<'a> for Never<T> {
    type Guard = tokio::sync::MutexGuard<'a, T>;
    fn into_inner(self) -> Self::Guard {
        panic!("This can't happen since it requires an instance of the never type");
    }
}

pub trait MutexImpl {
    type Guard<'a>: std::ops::Deref
    where
        Self: 'a;
    type LockError<'a>: LockError<'a, Guard = Self::Guard<'a>>
    where
        Self: 'a;

    const SUPPORTS_POISONING: bool;

    fn new() -> Self;
    fn lock(&self) -> Result<Self::Guard<'_>, Self::LockError<'_>>;
    fn try_lock(&self) -> Result<Self::Guard<'_>, std::sync::TryLockError<Self::Guard<'_>>>;
}

impl<T: Default> MutexImpl for std::sync::Mutex<T> {
    type Guard<'a>
    where
        T: 'a,
    = std::sync::MutexGuard<'a, T>;
    type LockError<'a>
    where
        T: 'a,
    = std::sync::PoisonError<Self::Guard<'a>>;

    const SUPPORTS_POISONING: bool = true;

    fn new() -> Self {
        std::sync::Mutex::new(T::default())
    }

    fn lock(&self) -> Result<Self::Guard<'_>, Self::LockError<'_>> {
        std::sync::Mutex::lock(self)
    }

    fn try_lock(&self) -> Result<Self::Guard<'_>, std::sync::TryLockError<Self::Guard<'_>>> {
        std::sync::Mutex::try_lock(self)
    }
}

#[cfg(feature = "tokio")]
impl<T: Default> MutexImpl for tokio::sync::Mutex<T> {
    type Guard<'a>
    where
        T: 'a,
    = tokio::sync::MutexGuard<'a, T>;
    type LockError<'a>
    where
        T: 'a,
    = Never<T>;

    const SUPPORTS_POISONING: bool = false;

    fn new() -> Self {
        tokio::sync::Mutex::new(T::default())
    }

    fn lock(&self) -> Result<Self::Guard<'_>, Self::LockError<'_>> {
        Ok(tokio::sync::Mutex::blocking_lock(self))
    }

    fn try_lock(&self) -> Result<Self::Guard<'_>, std::sync::TryLockError<Self::Guard<'_>>> {
        match tokio::sync::Mutex::try_lock(self) {
            Ok(ok) => Ok(ok),
            Err(_) => {
                // tokio::sync::Mutex::try_lock **only** fails if it would block, see https://docs.rs/tokio/latest/tokio/sync/struct.TryLockError.html
                Err(std::sync::TryLockError::WouldBlock)
            }
        }
    }
}
