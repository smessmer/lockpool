//! **This library is not maintained anymore. Please use the [lockable](https://crates.io/crates/lockable) crate instead.**
//!
//! **It offers a [LockPool](https://docs.rs/lockable/latest/lockable/struct.LockPool.html) data structure**
//! **with pretty much the same functionality that was offered by this crate.**
//! **There is an [example for a lock pool in the README](https://github.com/smessmer/lockable#lockpool-example).**
//!
//! This library offers a pool of locks where individual locks can be locked/unlocked by key.
//! It initially considers all keys as "unlocked", but they can be locked
//! and if a second thread tries to acquire a lock for the same key, they will have to wait.
//!
//! ```
//! use lockpool::{LockPool, SyncLockPool};
//!
//! let pool = SyncLockPool::new();
//! # (|| -> Result<(), lockpool::PoisonError<_, _>> {
//! let guard1 = pool.lock(4)?;
//! let guard2 = pool.lock(5)?;
//!
//! // This next line would cause a deadlock or panic because `4` is already locked on this thread
//! // let guard3 = pool.lock(4)?;
//!
//! // After dropping the corresponding guard, we can lock it again
//! std::mem::drop(guard1);
//! let guard3 = pool.lock(4)?;
//! # Ok(())
//! # })().unwrap();
//! ```
//!
//! You can use an arbitrary type to index locks by, as long as that type implements [PartialEq] + [Eq] + [Hash] + [Clone] + [Debug].
//!
//! ```
//! use lockpool::{LockPool, SyncLockPool};
//!
//! #[derive(PartialEq, Eq, Hash, Clone, Debug)]
//! struct CustomLockKey(u32);
//!
//! let pool = SyncLockPool::new();
//! # (|| -> Result<(), lockpool::PoisonError<_, _>> {
//! let guard = pool.lock(CustomLockKey(4))?;
//! # Ok(())
//! # })().unwrap();
//! ```
//!
//! Under the hood, a [LockPool] is a [HashMap](std::collections::HashMap) of [Mutex](std::sync::Mutex)es, with some logic making sure there aren't any race conditions when accessing the hash map.
//!
//! If the `tokio` feature is enabled, then this crate also offers [TokioLockPool] which allows locks to be held across `await` points.

#![deny(missing_docs)]

mod error;
mod guard;
mod mutex;
mod never;
mod pool;

pub use error::{PoisonError, TryLockError, UnpoisonError};
pub use guard::Guard;
pub use never::{InfallibleUnwrap, Never};
pub use pool::pool_sync::SyncLockPool;
pub use pool::LockPool;

#[cfg(feature = "tokio")]
pub use pool::pool_async::{AsyncLockPool, TokioLockPool};
