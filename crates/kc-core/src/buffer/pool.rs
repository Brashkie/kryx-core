//! A size-bucketed buffer pool for reusing allocations under pressure.
//!
//! In a pipeline that processes thousands of frames per second, allocating and
//! freeing a `BytesMut` for every frame keeps the allocator busy. This pool
//! recycles those allocations: [`BufferPool::acquire`] hands back a cleared
//! buffer of at least the requested size — reused from the pool when one fits,
//! freshly allocated otherwise — and [`BufferPool::recycle`] returns it for the
//! next caller.
//!
//! # Design
//!
//! - **Size-bucketed.** Buffers are grouped into power-of-two buckets (4 KiB,
//!   16 KiB, 64 KiB, ...). A request is served from the smallest bucket that
//!   fits, so a 50 KiB ask reuses a 64 KiB buffer. This avoids handing a 4 MiB
//!   buffer to a 4 KiB request (which a single free-list would do) and keeps
//!   fragmentation bounded.
//! - **Capped per bucket.** Each bucket retains at most `max_per_bucket`
//!   buffers. A temporary spike of 500 large frames does not pin their memory
//!   forever — extras are dropped on recycle and the allocator reclaims them.
//! - **Direct allocation above the largest bucket.** A giant one-off frame is
//!   allocated directly and, on recycle, dropped rather than retained — it never
//!   pollutes the pool. This keeps the pool's memory footprint predictable.
//! - **Explicit acquire/recycle.** The caller controls exactly when a buffer
//!   returns to the pool. No RAII guard, no hidden Drop cost — reuse and
//!   concurrency stay easy to reason about. (A guard type can be layered on top
//!   later without changing this primitive.)
//!
//! Whether this pool actually beats the allocator depends on frame sizes and
//! churn — that is what the benchmarks in `benches/` measure, rather than
//! something assumed here.
//!
//! # Example
//!
//! ```
//! use kc_core::buffer::BufferPool;
//!
//! let mut pool = BufferPool::new();
//! let mut buf = pool.acquire(50 * 1024); // served from the 64 KiB bucket
//! buf.extend_from_slice(b"frame data");
//! // ... use the buffer ...
//! pool.recycle(buf); // back to the pool for the next acquire
//! ```

use bytes::BytesMut;

/// Bucket boundaries, in bytes. A request is served by the first bucket whose
/// size is `>=` the request. Powers of two from 4 KiB to 4 MiB.
const BUCKET_SIZES: [usize; 11] = [
    4 * 1024,        // 4 KiB
    8 * 1024,        // 8 KiB
    16 * 1024,       // 16 KiB
    32 * 1024,       // 32 KiB
    64 * 1024,       // 64 KiB
    128 * 1024,      // 128 KiB
    256 * 1024,      // 256 KiB
    512 * 1024,      // 512 KiB
    1024 * 1024,     // 1 MiB
    2 * 1024 * 1024, // 2 MiB
    4 * 1024 * 1024, // 4 MiB
];

/// The largest size served from a bucket. Requests above this are allocated
/// directly and dropped on recycle (never retained).
const MAX_BUCKET_SIZE: usize = 4 * 1024 * 1024;

/// Default cap on retained buffers per bucket.
const DEFAULT_MAX_PER_BUCKET: usize = 32;

/// A size-bucketed pool of reusable byte buffers.
///
/// Not thread-safe by itself — wrap it in a `Mutex` or give each worker its own
/// pool. Single-owner-per-pool is the common and fastest pattern in a pipeline
/// stage, so the primitive stays lock-free.
pub struct BufferPool {
    /// One free-list per bucket, parallel to [`BUCKET_SIZES`].
    buckets: [Vec<BytesMut>; BUCKET_SIZES.len()],
    /// Max buffers retained per bucket.
    max_per_bucket: usize,
    /// Cumulative stats for observability / benchmarking.
    stats: PoolStats,
}

/// Counters describing pool behavior. Useful in benchmarks and metrics to see
/// how often the pool actually saved an allocation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PoolStats {
    /// Acquires served by reusing a pooled buffer.
    pub hits: u64,
    /// Acquires that had to allocate (empty bucket or oversized request).
    pub misses: u64,
    /// Recycled buffers accepted back into a bucket.
    pub recycled: u64,
    /// Recycled buffers dropped (bucket full, or oversized).
    pub dropped: u64,
}

impl PoolStats {
    /// Fraction of acquires served from the pool, in `[0.0, 1.0]`.
    /// Returns 0.0 before any acquire.
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

impl BufferPool {
    /// Create a pool with the default per-bucket cap (32).
    pub fn new() -> Self {
        Self::with_max_per_bucket(DEFAULT_MAX_PER_BUCKET)
    }

    /// Create a pool retaining at most `max_per_bucket` buffers per bucket.
    ///
    /// A cap of 0 disables retention — every recycle is dropped, turning the
    /// pool into a plain allocator (useful as a benchmark baseline).
    pub fn with_max_per_bucket(max_per_bucket: usize) -> Self {
        Self {
            // `Vec` is not `Copy`, so build the array element by element.
            buckets: std::array::from_fn(|_| Vec::new()),
            max_per_bucket,
            stats: PoolStats::default(),
        }
    }

    /// Index of the smallest bucket that can serve `size`, or `None` if `size`
    /// exceeds the largest bucket (direct-allocation territory).
    #[inline]
    fn bucket_index(size: usize) -> Option<usize> {
        if size > MAX_BUCKET_SIZE {
            return None;
        }
        // First bucket whose capacity is >= size.
        BUCKET_SIZES.iter().position(|&b| b >= size)
    }

    /// Acquire a buffer with capacity for at least `size` bytes.
    ///
    /// The returned buffer is empty (`len == 0`) but has sufficient capacity —
    /// reused from the pool when one is available, freshly allocated otherwise.
    /// Requests larger than the biggest bucket are allocated directly.
    pub fn acquire(&mut self, size: usize) -> BytesMut {
        match Self::bucket_index(size) {
            Some(idx) => {
                if let Some(mut buf) = self.buckets[idx].pop() {
                    buf.clear(); // reset len to 0, keep capacity
                    self.stats.hits += 1;
                    buf
                } else {
                    self.stats.misses += 1;
                    BytesMut::with_capacity(BUCKET_SIZES[idx])
                }
            }
            None => {
                // Oversized: allocate exactly what's asked, never pooled.
                self.stats.misses += 1;
                BytesMut::with_capacity(size)
            }
        }
    }

    /// Return a buffer to the pool for reuse.
    ///
    /// The buffer is filed into the bucket matching its *capacity*. It is
    /// dropped instead of retained when the bucket is full, or when its capacity
    /// exceeds the largest bucket. Its contents are irrelevant — `acquire`
    /// clears length before handing a buffer back.
    pub fn recycle(&mut self, buf: BytesMut) {
        let cap = buf.capacity();
        // File by the largest bucket whose size fits WITHIN this capacity, so a
        // buffer is only reused for requests it can actually satisfy. A buffer
        // with capacity between two bucket sizes files into the lower one.
        let idx = match Self::recycle_bucket_index(cap) {
            Some(idx) => idx,
            None => {
                // Capacity below the smallest bucket, or above the largest.
                self.stats.dropped += 1;
                return;
            }
        };

        if self.buckets[idx].len() < self.max_per_bucket {
            self.buckets[idx].push(buf);
            self.stats.recycled += 1;
        } else {
            self.stats.dropped += 1;
        }
    }

    /// Bucket a buffer of the given capacity belongs in when recycled: the
    /// largest bucket whose size is `<=` the capacity. `None` if the capacity is
    /// smaller than the smallest bucket or larger than the largest.
    #[inline]
    fn recycle_bucket_index(cap: usize) -> Option<usize> {
        if cap > MAX_BUCKET_SIZE || cap < BUCKET_SIZES[0] {
            return None;
        }
        // rposition: largest bucket size that is <= cap.
        BUCKET_SIZES.iter().rposition(|&b| b <= cap)
    }

    /// A snapshot of the pool's cumulative statistics.
    pub fn stats(&self) -> PoolStats {
        self.stats
    }

    /// Total buffers currently retained across all buckets.
    pub fn pooled_count(&self) -> usize {
        self.buckets.iter().map(Vec::len).sum()
    }

    /// Pre-warm the pool: allocate `count` buffers sized for `size` and file them
    /// into the matching bucket, so the first `count` acquires of that size are
    /// hits instead of misses.
    ///
    /// A pipeline that will process frames of a known size can call this at
    /// startup to pay the allocation cost up front (and fault the pages in)
    /// rather than during the first frames, when latency usually matters most.
    /// Respects the per-bucket cap: requests beyond the cap are not allocated.
    /// Requests for a size above the largest bucket are ignored (those are never
    /// pooled). Prewarmed buffers count as `recycled` in the stats.
    pub fn reserve(&mut self, size: usize, count: usize) {
        let idx = match Self::bucket_index(size) {
            Some(idx) => idx,
            None => return, // oversized — never pooled
        };
        let capacity = BUCKET_SIZES[idx];
        let room = self.max_per_bucket.saturating_sub(self.buckets[idx].len());
        for _ in 0..count.min(room) {
            self.buckets[idx].push(BytesMut::with_capacity(capacity));
            self.stats.recycled += 1;
        }
    }

    /// Total capacity, in bytes, currently retained by pooled buffers.
    ///
    /// This is the memory the pool is holding onto for reuse — the number to
    /// watch for the pool's footprint, distinct from [`pooled_count`] (how many
    /// buffers) and from the hit-rate stats (how effective). Computed from each
    /// bucket's size times how many buffers it holds.
    ///
    /// [`pooled_count`]: Self::pooled_count
    pub fn memory_used(&self) -> usize {
        self.buckets
            .iter()
            .enumerate()
            .map(|(idx, bucket)| bucket.len() * BUCKET_SIZES[idx])
            .sum()
    }

    /// Drop every retained buffer, releasing their memory. Stats are kept.
    pub fn clear(&mut self) {
        for bucket in &mut self.buckets {
            bucket.clear();
        }
    }
}

impl Default for BufferPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_gives_sufficient_capacity() {
        let mut pool = BufferPool::new();
        let buf = pool.acquire(50 * 1024);
        // 50 KiB → 64 KiB bucket.
        assert!(buf.capacity() >= 50 * 1024);
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn recycle_then_acquire_reuses_same_allocation() {
        let mut pool = BufferPool::new();
        let mut buf = pool.acquire(4 * 1024);
        buf.extend_from_slice(&[1, 2, 3]);
        let ptr = buf.as_ptr();
        pool.recycle(buf);

        let reused = pool.acquire(4 * 1024);
        // Same underlying allocation came back, and it's cleared.
        assert_eq!(reused.as_ptr(), ptr);
        assert_eq!(reused.len(), 0);
        assert_eq!(pool.stats().hits, 1);
    }

    #[test]
    fn size_bucketing_routes_to_correct_bucket() {
        let mut pool = BufferPool::new();
        // Acquire and recycle a 64 KiB buffer.
        let buf = pool.acquire(60 * 1024); // → 64 KiB bucket
        assert!(buf.capacity() >= 64 * 1024);
        pool.recycle(buf);
        // A 50 KiB request should reuse the pooled 64 KiB buffer.
        let reused = pool.acquire(50 * 1024);
        assert_eq!(pool.stats().hits, 1);
        assert!(reused.capacity() >= 50 * 1024);
    }

    #[test]
    fn oversized_request_is_direct_and_not_pooled() {
        let mut pool = BufferPool::new();
        let big = 8 * 1024 * 1024; // > 4 MiB max bucket
        let buf = pool.acquire(big);
        assert!(buf.capacity() >= big);
        assert_eq!(pool.stats().misses, 1);
        pool.recycle(buf);
        // Oversized buffers are dropped, not retained.
        assert_eq!(pool.pooled_count(), 0);
        assert_eq!(pool.stats().dropped, 1);
    }

    #[test]
    fn per_bucket_cap_bounds_retention() {
        let mut pool = BufferPool::with_max_per_bucket(2);
        // Acquire all three FIRST — otherwise the next acquire would just pull
        // back the buffer we recycled, and the bucket would never accumulate.
        // Then recycle all three; only two fit, the third is dropped.
        let a = pool.acquire(4 * 1024);
        let b = pool.acquire(4 * 1024);
        let c = pool.acquire(4 * 1024);
        pool.recycle(a);
        pool.recycle(b);
        pool.recycle(c); // bucket full at 2 → dropped
        assert_eq!(pool.pooled_count(), 2);
        assert_eq!(pool.stats().dropped, 1);
    }

    #[test]
    fn max_per_bucket_zero_disables_pooling() {
        let mut pool = BufferPool::with_max_per_bucket(0);
        let buf = pool.acquire(4 * 1024);
        pool.recycle(buf);
        assert_eq!(pool.pooled_count(), 0);
        // Next acquire must allocate — no reuse.
        let _ = pool.acquire(4 * 1024);
        assert_eq!(pool.stats().hits, 0);
        assert_eq!(pool.stats().misses, 2);
    }

    #[test]
    fn hit_rate_reflects_reuse() {
        let mut pool = BufferPool::new();
        let b = pool.acquire(4 * 1024); // miss
        pool.recycle(b);
        let _ = pool.acquire(4 * 1024); // hit
        let stats = pool.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert!((stats.hit_rate() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn tiny_buffer_below_smallest_bucket_is_dropped_on_recycle() {
        let mut pool = BufferPool::new();
        // Manually make a sub-4-KiB buffer (acquire would round up, so build direct).
        let small = BytesMut::with_capacity(1024);
        pool.recycle(small);
        assert_eq!(pool.pooled_count(), 0);
        assert_eq!(pool.stats().dropped, 1);
    }

    #[test]
    fn clear_releases_retained_buffers() {
        let mut pool = BufferPool::new();
        // Acquire all three first, THEN recycle all three, so they accumulate in
        // the bucket (recycling one at a time in a loop wouldn't — the next
        // acquire would pull it straight back out).
        let a = pool.acquire(4 * 1024);
        let b = pool.acquire(4 * 1024);
        let c = pool.acquire(4 * 1024);
        pool.recycle(a);
        pool.recycle(b);
        pool.recycle(c);
        assert_eq!(pool.pooled_count(), 3);
        pool.clear();
        assert_eq!(pool.pooled_count(), 0);
    }

    #[test]
    fn reserve_prewarms_and_turns_first_acquires_into_hits() {
        let mut pool = BufferPool::new();
        pool.reserve(64 * 1024, 4);
        assert_eq!(pool.pooled_count(), 4);
        assert_eq!(pool.stats().recycled, 4);
        // The next acquires of that size are hits, not misses.
        let _ = pool.acquire(60 * 1024); // → 64 KiB bucket
        assert_eq!(pool.stats().hits, 1);
        assert_eq!(pool.stats().misses, 0);
    }

    #[test]
    fn reserve_respects_bucket_cap() {
        let mut pool = BufferPool::with_max_per_bucket(2);
        pool.reserve(4 * 1024, 10); // asks 10, cap is 2
        assert_eq!(pool.pooled_count(), 2);
    }

    #[test]
    fn reserve_ignores_oversized() {
        let mut pool = BufferPool::new();
        pool.reserve(8 * 1024 * 1024, 3); // > 4 MiB max bucket
        assert_eq!(pool.pooled_count(), 0);
    }

    #[test]
    fn memory_used_reports_retained_bytes() {
        let mut pool = BufferPool::new();
        assert_eq!(pool.memory_used(), 0);
        pool.reserve(64 * 1024, 3); // three 64 KiB buffers
        assert_eq!(pool.memory_used(), 3 * 64 * 1024);
        pool.reserve(1024 * 1024, 1); // + one 1 MiB
        assert_eq!(pool.memory_used(), 3 * 64 * 1024 + 1024 * 1024);
        pool.clear();
        assert_eq!(pool.memory_used(), 0);
    }
}
