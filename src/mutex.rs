pub trait LockError<'a> {
    type Guard;
    fn into_inner(self) -> Self::Guard;
}

impl<'a> LockError<'a> for std::sync::PoisonError<std::sync::MutexGuard<'a, ()>> {
    type Guard = std::sync::MutexGuard<'a, ()>;
    fn into_inner(self) -> Self::Guard {
        std::sync::PoisonError::into_inner(self)
    }
}

#[cfg(feature = "tokio")]
impl<'a> LockError<'a> for ! {
    type Guard = tokio::sync::MutexGuard<'a, ()>;
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

    fn new() -> Self;
    fn lock(&self) -> Result<Self::Guard<'_>, Self::LockError<'_>>;
    fn try_lock(&self) -> Result<Self::Guard<'_>, std::sync::TryLockError<Self::Guard<'_>>>;
}

impl MutexImpl for std::sync::Mutex<()> {
    type Guard<'a> = std::sync::MutexGuard<'a, ()>;
    type LockError<'a> = std::sync::PoisonError<Self::Guard<'a>>;

    fn new() -> Self {
        std::sync::Mutex::new(())
    }

    fn lock(&self) -> Result<Self::Guard<'_>, Self::LockError<'_>> {
        std::sync::Mutex::lock(self)
    }

    fn try_lock(&self) -> Result<Self::Guard<'_>, std::sync::TryLockError<Self::Guard<'_>>> {
        std::sync::Mutex::try_lock(self)
    }
}

#[cfg(feature = "tokio")]
impl MutexImpl for tokio::sync::Mutex<()> {
    type Guard<'a> = tokio::sync::MutexGuard<'a, ()>;
    type LockError<'a> = !; // TODO Should be ! instead of ()

    fn new() -> Self {
        tokio::sync::Mutex::new(())
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
