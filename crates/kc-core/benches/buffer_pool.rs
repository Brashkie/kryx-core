//! Benchmarks: `BufferPool` vs. direct allocation.
//!
//! The whole point of v0.2.0 is to *measure*, not assume, that pooling helps.
//! These benches compare the baseline (a fresh `BytesMut` per frame, dropped
//! after use) against the pool (acquire / use / recycle) across three axes the
//! pool's value depends on:
//!
//! - **frame size** — small frames (4 KiB) vs large (1 MiB). The allocator is
//!   very good at small objects, so the pool may only win at larger sizes; the
//!   bench will show where the crossover is.
//! - **churn** — a tight acquire/recycle loop, the pool's best case.
//! - **mixed load** — a realistic spread of sizes, closer to a real pipeline.
//!
//! Run with `cargo bench`. Compare the `baseline/*` and `pool/*` lines: if the
//! pool doesn't beat the allocator for a given shape, that's a real result —
//! it tells us which sizes are worth pooling and which aren't.

use bytes::BytesMut;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use kc_core::buffer::BufferPool;

/// A fixed block of bytes to write into each buffer, so the allocation is
/// actually touched (an untouched allocation can be optimized away and wouldn't
/// be realistic). 64 bytes is enough to fault in the first page.
const TOUCH_BYTES: [u8; 64] = [0xAB; 64];

/// Write a few bytes into the buffer so its memory is really used.
#[inline]
fn touch(buf: &mut BytesMut, size: usize) {
    let n = size.min(TOUCH_BYTES.len());
    buf.extend_from_slice(&TOUCH_BYTES[..n]);
}

fn bench_single_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("single_size");
    for &size in &[4 * 1024usize, 64 * 1024, 1024 * 1024] {
        group.throughput(Throughput::Bytes(size as u64));

        // Baseline: allocate a fresh buffer each iteration, drop it.
        group.bench_with_input(BenchmarkId::new("baseline", size), &size, |b, &size| {
            b.iter(|| {
                let mut buf = BytesMut::with_capacity(size);
                touch(&mut buf, size);
                black_box(&buf);
                // buf dropped here — allocation freed
            });
        });

        // Pool: acquire / touch / recycle each iteration.
        group.bench_with_input(BenchmarkId::new("pool", size), &size, |b, &size| {
            let mut pool = BufferPool::new();
            // Warm the pool so we measure steady-state reuse, not the first miss.
            let warm = pool.acquire(size);
            pool.recycle(warm);
            b.iter(|| {
                let mut buf = pool.acquire(size);
                touch(&mut buf, size);
                black_box(&buf);
                pool.recycle(buf);
            });
        });
    }
    group.finish();
}

fn bench_churn(c: &mut Criterion) {
    // The pool's best case: many acquire/recycle cycles at one size.
    let mut group = c.benchmark_group("churn_64k");
    const SIZE: usize = 64 * 1024;
    const N: usize = 256;

    group.bench_function("baseline", |b| {
        b.iter(|| {
            for _ in 0..N {
                let mut buf = BytesMut::with_capacity(SIZE);
                touch(&mut buf, SIZE);
                black_box(&buf);
            }
        });
    });

    group.bench_function("pool", |b| {
        let mut pool = BufferPool::new();
        b.iter(|| {
            for _ in 0..N {
                let mut buf = pool.acquire(SIZE);
                touch(&mut buf, SIZE);
                black_box(&buf);
                pool.recycle(buf);
            }
        });
    });
    group.finish();
}

fn bench_mixed(c: &mut Criterion) {
    // A realistic spread: mostly small frames, occasional large ones.
    let sizes = [
        4 * 1024,
        4 * 1024,
        16 * 1024,
        4 * 1024,
        256 * 1024,
        64 * 1024,
        4 * 1024,
    ];
    let mut group = c.benchmark_group("mixed_load");

    group.bench_function("baseline", |b| {
        b.iter(|| {
            for &size in &sizes {
                let mut buf = BytesMut::with_capacity(size);
                touch(&mut buf, size);
                black_box(&buf);
            }
        });
    });

    group.bench_function("pool", |b| {
        let mut pool = BufferPool::new();
        b.iter(|| {
            for &size in &sizes {
                let mut buf = pool.acquire(size);
                touch(&mut buf, size);
                black_box(&buf);
                pool.recycle(buf);
            }
        });
    });
    group.finish();
}

criterion_group!(benches, bench_single_size, bench_churn, bench_mixed);
criterion_main!(benches);
