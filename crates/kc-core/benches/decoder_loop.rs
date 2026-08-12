//! End-to-end benchmark: the pool in a decoder-like frame loop.
//!
//! The microbenchmarks in `buffer_pool.rs` measure the pool in isolation
//! (acquire → touch → recycle, 100% hit rate). This one measures it the way a
//! real decoder would use it: build a `MediaBufferMut`, write a frame's worth of
//! bytes, `freeze()` into a `MediaBuffer`, and move on to the next frame — over a
//! whole sequence, so the pool's steady-state hit rate is what actually shows up.
//!
//! Two things this reveals that the micro-bench can't:
//!
//! 1. **Realistic hit rate.** A decoder freezes each frame into an owned
//!    `MediaBuffer` that lives on (goes to the next stage). With `freeze()` the
//!    scratch returns to the pool, so the *next* frame reuses it — the hit rate
//!    approaches 100% for a steady frame size, but the bench measures it rather
//!    than assuming it, via `PoolStats::hit_rate()`.
//! 2. **The copy cost.** `freeze()` copies scratch → owned `Bytes`. That copy is
//!    real work the pool does NOT remove. So the end-to-end win is smaller than
//!    the micro-bench's (which never copied) — this bench shows the honest,
//!    decoder-shaped number.
//!
//! Architectural note discovered here: `SharedPool` is `Rc<RefCell<_>>`, i.e.
//! single-threaded. It cannot be a field of a `Stage` (which is `Send + Sync`),
//! so this bench drives the pool synchronously — a faithful model of one decoder
//! worker, and a data point for deciding v0.3's threading model.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use kc_core::buffer::{shared_pool, MediaBufferMut};
use kc_core::types::MediaType;

/// Simulate producing one frame's payload of `size` bytes.
/// A real decoder would write actual samples; we write a repeating pattern so
/// the allocation is genuinely touched (pages faulted in).
#[inline]
fn fill_frame(buf: &mut MediaBufferMut, size: usize) {
    const CHUNK: [u8; 256] = [0x5A; 256];
    let mut remaining = size;
    while remaining > 0 {
        let n = remaining.min(CHUNK.len());
        buf.extend(&CHUNK[..n]);
        remaining -= n;
    }
}

/// A run of frames at one size — the shape a steady decoder produces.
const FRAMES: usize = 128;

fn bench_decoder_loop(c: &mut Criterion) {
    let mut group = c.benchmark_group("decoder_loop");

    for &size in &[8 * 1024usize, 64 * 1024, 512 * 1024] {
        group.throughput(Throughput::Bytes((size * FRAMES) as u64));

        // Baseline: each frame allocates a fresh MediaBufferMut, freezes, drops.
        group.bench_with_input(BenchmarkId::new("no_pool", size), &size, |b, &size| {
            b.iter(|| {
                for _ in 0..FRAMES {
                    let mut buf = MediaBufferMut::with_capacity(MediaType::Video, size);
                    fill_frame(&mut buf, size);
                    let frame = buf.freeze().unwrap();
                    black_box(&frame);
                    // frame dropped here — its owned allocation freed
                }
            });
        });

        // Pooled: each frame draws scratch from the pool; freeze() copies out and
        // recycles the scratch, so the next frame reuses it.
        group.bench_with_input(BenchmarkId::new("pool", size), &size, |b, &size| {
            let pool = shared_pool();
            b.iter(|| {
                for _ in 0..FRAMES {
                    let mut buf = MediaBufferMut::with_pool(&pool, MediaType::Video, size);
                    fill_frame(&mut buf, size);
                    let frame = buf.freeze().unwrap();
                    black_box(&frame);
                }
            });
        });
    }
    group.finish();
}

/// Report the steady-state hit rate for a decoder-shaped loop, so the benchmark
/// output includes the actual reuse fraction (not just timings). This runs once
/// and prints; it isn't a timing bench.
fn report_hit_rate(_c: &mut Criterion) {
    for &size in &[8 * 1024usize, 64 * 1024, 512 * 1024] {
        let pool = shared_pool();
        for _ in 0..FRAMES {
            let mut buf = MediaBufferMut::with_pool(&pool, MediaType::Video, size);
            fill_frame(&mut buf, size);
            let _ = buf.freeze().unwrap();
        }
        let stats = pool.borrow().stats();
        eprintln!(
            "hit_rate[{:>7} B]: {:.1}%  (hits={}, misses={}, recycled={})",
            size,
            stats.hit_rate() * 100.0,
            stats.hits,
            stats.misses,
            stats.recycled,
        );
    }
}

criterion_group!(benches, bench_decoder_loop, report_hit_rate);
criterion_main!(benches);
