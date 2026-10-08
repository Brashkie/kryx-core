//! Per-worker buffer pooling — the concurrency pattern the threading benchmark
//! chose (v0.2.5 `benches/threading.rs`).
//!
//! The benchmark compared three ways to use a [`BufferPool`] across 4 concurrent
//! workers. A single `Arc<Mutex<BufferPool>>` *lost to not pooling at all* on
//! small frames (the lock cost more than the allocation it saved), and only tied
//! on large ones. A private pool per worker won at every size and never lost.
//!
//! So core does not offer a shared, lock-guarded pool. It offers this: each
//! worker thread gets its own [`BufferPool`], with no lock and no cross-thread
//! sharing. That matches the pool's original lock-free design — the fastest pool
//! is the one no other thread can touch.
//!
//! [`WorkerPool`] is a zero-sized handle over a `thread_local!` pool: clone it,
//! move it into each worker, and every thread transparently uses its own pool.
//! The handle is `Send + Sync` (it owns no data), so unlike [`SharedPool`] it can
//! live inside a `Send + Sync` `Stage`.
//!
//! [`SharedPool`]: crate::buffer::SharedPool
//!
//! # Example
//!
//! ```
//! use kc_core::buffer::WorkerPool;
//!
//! let wp = WorkerPool::new();
//! // In each worker thread, draw and return buffers from *that thread's* pool:
//! let mut scratch = wp.acquire(64 * 1024);
//! scratch.extend_from_slice(b"frame data");
//! scratch.clear();
//! wp.recycle(scratch);
//! // This thread's pool now has one buffer ready to reuse.
//! assert_eq!(wp.with(|p| p.pooled_count()), 1);
//! ```

use std::cell::RefCell;

use bytes::BytesMut;

use super::pool::{BufferPool, PoolStats};

thread_local! {
    /// One pool per thread. Lazily created the first time a thread touches it.
    static WORKER_POOL: RefCell<BufferPool> = RefCell::new(BufferPool::new());
}

/// A `Send + Sync` handle to a per-thread [`BufferPool`].
///
/// Every operation runs against the pool belonging to *the calling thread*, so
/// there is never contention between workers. Cloning is free (the handle holds
/// no data); give each worker a clone.
#[derive(Debug, Clone, Copy, Default)]
pub struct WorkerPool {
    _private: (),
}

impl WorkerPool {
    /// Create a handle to the per-worker pools.
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Acquire a buffer of at least `size` bytes from the current thread's pool.
    pub fn acquire(&self, size: usize) -> BytesMut {
        WORKER_POOL.with(|p| p.borrow_mut().acquire(size))
    }

    /// Return a buffer to the current thread's pool for reuse.
    ///
    /// Clear it first if you don't want its old contents counted toward length;
    /// the pool reuses the allocation regardless.
    pub fn recycle(&self, buf: BytesMut) {
        WORKER_POOL.with(|p| p.borrow_mut().recycle(buf));
    }

    /// Pre-warm the current thread's pool with `count` buffers sized for `size`,
    /// so this worker's first acquires are hits. Call once per worker at startup.
    pub fn reserve(&self, size: usize, count: usize) {
        WORKER_POOL.with(|p| p.borrow_mut().reserve(size, count));
    }

    /// Stats for the current thread's pool (each worker has its own counters).
    pub fn stats(&self) -> PoolStats {
        WORKER_POOL.with(|p| p.borrow().stats())
    }

    /// Run a closure with mutable access to the current thread's pool, for
    /// operations not surfaced directly here (e.g. `shrink_to`, `memory_used`).
    pub fn with<R>(&self, f: impl FnOnce(&mut BufferPool) -> R) -> R {
        WORKER_POOL.with(|p| f(&mut p.borrow_mut()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn acquire_recycle_uses_this_threads_pool() {
        let wp = WorkerPool::new();
        let mut b = wp.acquire(64 * 1024);
        b.extend_from_slice(&[1, 2, 3]);
        b.clear();
        wp.recycle(b);
        assert_eq!(wp.with(|p| p.pooled_count()), 1);
        // Reuse shows as a hit on this thread.
        let _ = wp.acquire(60 * 1024);
        assert_eq!(wp.stats().hits, 1);
    }

    #[test]
    fn each_thread_has_an_independent_pool() {
        let wp = WorkerPool::new();
        // Main thread recycles one buffer.
        let b = wp.acquire(4 * 1024);
        wp.recycle(b);
        assert_eq!(wp.with(|p| p.pooled_count()), 1);

        // A fresh thread starts with an empty pool — no sharing.
        let wp2 = wp;
        let pooled_in_worker = thread::spawn(move || {
            let before = wp2.with(|p| p.pooled_count());
            let b = wp2.acquire(4 * 1024);
            wp2.recycle(b);
            (before, wp2.with(|p| p.pooled_count()))
        })
        .join()
        .unwrap();
        assert_eq!(pooled_in_worker, (0, 1)); // started empty, then its own 1
    }

    #[test]
    fn reserve_prewarms_the_worker_pool() {
        let wp = WorkerPool::new();
        thread::spawn(move || {
            wp.reserve(16 * 1024, 4);
            assert_eq!(wp.with(|p| p.pooled_count()), 4);
            let _ = wp.acquire(16 * 1024);
            assert_eq!(wp.stats().hits, 1);
            assert_eq!(wp.stats().misses, 0);
        })
        .join()
        .unwrap();
    }
}
