use criterion::{black_box, criterion_group, criterion_main, Criterion};
use crossbeam_utils::thread;
#[cfg(feature = "tokio")]
use lockpool::AsyncLockPool;
use lockpool::{LockPool, SyncLockPool};
use std::sync::{Arc, Mutex};

pub fn single_thread_lock_unlock(c: &mut Criterion) {
    let mut g = c.benchmark_group("single thread lock unlock");
    g.bench_function("std Mutex", |b| {
        let mutex = Mutex::new(());
        b.iter(|| {
            let _g = mutex.lock().unwrap();
        })
    });
    #[cfg(feature = "tokio")]
    g.bench_function("tokio Mutex", |b| {
        let mutex = tokio::sync::Mutex::new(());
        b.iter(|| {
            let _g = mutex.blocking_lock();
        })
    });
    g.bench_function("SyncLockPool (same key)", |b| {
        let pool = SyncLockPool::new();
        b.iter(|| {
            let _g = pool.lock(black_box(3)).unwrap();
        })
    });
    #[cfg(feature = "tokio")]
    g.bench_function("AsyncLockPool (same key)", |b| {
        let pool = AsyncLockPool::new();
        b.iter(|| {
            let _g = pool.lock(black_box(3)).unwrap();
        })
    });
    g.bench_function("SyncLockPool (different key)", |b| {
        let pool = SyncLockPool::new();
        let mut i = 0;
        b.iter(|| {
            i += 1;
            let _g = pool.lock(black_box(i)).unwrap();
        })
    });
    #[cfg(feature = "tokio")]
    g.bench_function("AsyncLockPool (different key)", |b| {
        let pool = AsyncLockPool::new();
        let mut i = 0;
        b.iter(|| {
            i += 1;
            let _g = pool.lock(black_box(i)).unwrap();
        })
    });
    g.finish();
}

fn spawn_threads(num: usize, func: impl Fn(usize) + Send + Sync) {
    thread::scope(|s| {
        for _ in 0..num {
            s.spawn(|_| func(num));
        }
    })
    .unwrap();
}

pub fn multi_thread_lock_unlock(c: &mut Criterion) {
    const NUM_THREADS: usize = 500;
    const NUM_LOCKS_PER_THREAD: usize = 1000;

    let mut g = c.benchmark_group("multi thread lock unlock");
    g.bench_function("std Mutex", |b| {
        let mutex = Arc::new(Mutex::new(()));
        b.iter(move || {
            spawn_threads(NUM_THREADS, |_| {
                for _ in 0..NUM_LOCKS_PER_THREAD {
                    let _g = mutex.lock().unwrap();
                }
            });
        })
    });
    #[cfg(feature = "tokio")]
    g.bench_function("tokio Mutex", |b| {
        let mutex = Arc::new(tokio::sync::Mutex::new(()));
        b.iter(move || {
            spawn_threads(NUM_THREADS, |_| {
                for _ in 0..NUM_LOCKS_PER_THREAD {
                    let _g = mutex.blocking_lock();
                }
            });
        })
    });
    g.bench_function("SyncLockPool (same key)", |b| {
        let pool = SyncLockPool::new();
        b.iter(move || {
            spawn_threads(NUM_THREADS, |_| {
                for _ in 0..NUM_LOCKS_PER_THREAD {
                    let _g = pool.lock(black_box(3)).unwrap();
                }
            });
        })
    });
    #[cfg(feature = "tokio")]
    g.bench_function("AsyncLockPool (same key)", |b| {
        let pool = AsyncLockPool::new();
        b.iter(move || {
            spawn_threads(NUM_THREADS, |_| {
                for _ in 0..NUM_LOCKS_PER_THREAD {
                    let _g = pool.lock(black_box(3)).unwrap();
                }
            });
        })
    });
    g.bench_function("SyncLockPool (different key)", |b| {
        let pool = SyncLockPool::new();
        b.iter(move || {
            spawn_threads(NUM_THREADS, |thread_index| {
                for _ in 0..NUM_LOCKS_PER_THREAD {
                    let _g = pool.lock(black_box(thread_index)).unwrap();
                }
            });
        })
    });
    #[cfg(feature = "tokio")]
    g.bench_function("AsyncLockPool (different key)", |b| {
        let pool = AsyncLockPool::new();
        b.iter(move || {
            spawn_threads(NUM_THREADS, |thread_index| {
                for _ in 0..NUM_LOCKS_PER_THREAD {
                    let _g = pool.lock(black_box(thread_index)).unwrap();
                }
            });
        })
    });
    g.finish();
}

criterion_group!(benches, single_thread_lock_unlock, multi_thread_lock_unlock,);
criterion_main!(benches);
