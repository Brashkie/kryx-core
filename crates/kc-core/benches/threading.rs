//! Threading benchmark: how should the pool be shared across workers?
//!
//! The pool won big in the single-threaded benches (v0.2.0/0.2.2), but
//! `SharedPool` is `Rc<RefCell<_>>` — single-threaded, so it can't live inside a
//! `Send + Sync` `Stage`. Before committing core to a threading model, this bench
//! measures the real candidates under concurrent load, so the decision is made
//! with numbers rather than preemptively (the v0.2.2 finding, now resolved here).
//!
//! Three strategies, `WORKERS` threads each processing `FRAMES` frames:
//!
//! 1. **`no_pool`** — every frame allocates a fresh `BytesMut` and drops it. This
//!    is what a `Stage` does today. The baseline to beat.
//! 2. **`arc_mutex`** — one `Arc<Mutex<BufferPool>>` shared by all workers. Simple
//!    and `Send + Sync`, but every acquire/recycle takes the lock, so workers
//!    contend. The question is whether the pool's savings survive that contention.
//! 3. **`thread_local`** — each worker owns a private `BufferPool` (no lock). Full
//!    pool speed, no contention, but N pools means N× the retained memory, and it
//!    only helps if a worker reuses its own buffers (steady per-worker size).
//!
//! What the numbers decide: if `arc_mutex` still beats `no_pool` handily, a
//! `Send + Sync` shared pool is worth adding to core. If the lock eats the gains
//! and only `thread_local` wins, core should offer a per-worker pool pattern
//! instead. If neither beats `no_pool` under contention, the pool stays a
//! single-threaded tool and stages allocate — an honest, measured answer either
//! way.

use std::sync::{Arc, Mutex};
use std::thread;

use bytes::BytesMut;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use kc_core::buffer::BufferPool;

const WORKERS: usize = 4;
const FRAMES: usize = 256;

/// Touch the buffer so the allocation is real (pages faulted in).
#[inline]
fn touch(buf: &mut BytesMut, size: usize) {
    const CHUNK: [u8; 256] = [0x5A; 256];
    let mut remaining = size;
    while remaining > 0 {
        let n = remaining.min(CHUNK.len());
        buf.extend_from_slice(&CHUNK[..n]);
        remaining -= n;
    }
}

/// Baseline: each worker allocates a fresh buffer per frame (no pool).
fn run_no_pool(size: usize) {
    let handles: Vec<_> = (0..WORKERS)
        .map(|_| {
            thread::spawn(move || {
                for _ in 0..FRAMES {
                    let mut buf = BytesMut::with_capacity(size);
                    touch(&mut buf, size);
                    black_box(&buf);
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
}

/// One pool behind a mutex, shared by all workers.
fn run_arc_mutex(size: usize) {
    let pool = Arc::new(Mutex::new(BufferPool::new()));
    let handles: Vec<_> = (0..WORKERS)
        .map(|_| {
            let pool = Arc::clone(&pool);
            thread::spawn(move || {
                for _ in 0..FRAMES {
                    // Lock only around acquire/recycle, not around the work.
                    let mut buf = pool.lock().unwrap().acquire(size);
                    touch(&mut buf, size);
                    black_box(&buf);
                    buf.clear();
                    pool.lock().unwrap().recycle(buf);
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
}

/// Each worker owns a private pool — no lock, no sharing.
fn run_thread_local(size: usize) {
    let handles: Vec<_> = (0..WORKERS)
        .map(|_| {
            thread::spawn(move || {
                let mut pool = BufferPool::new();
                for _ in 0..FRAMES {
                    let mut buf = pool.acquire(size);
                    touch(&mut buf, size);
                    black_box(&buf);
                    buf.clear();
                    pool.recycle(buf);
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
}

fn bench_threading(c: &mut Criterion) {
    let mut group = c.benchmark_group("threading");
    // Spawning threads dominates tiny inputs; keep the sample size modest so the
    // bench finishes in reasonable time while staying stable.
    group.sample_size(30);

    for &size in &[16 * 1024usize, 256 * 1024] {
        group.bench_with_input(BenchmarkId::new("no_pool", size), &size, |b, &size| {
            b.iter(|| run_no_pool(size));
        });
        group.bench_with_input(BenchmarkId::new("arc_mutex", size), &size, |b, &size| {
            b.iter(|| run_arc_mutex(size));
        });
        group.bench_with_input(BenchmarkId::new("thread_local", size), &size, |b, &size| {
            b.iter(|| run_thread_local(size));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_threading);
criterion_main!(benches);
