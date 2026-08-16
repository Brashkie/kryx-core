# Roadmap

`@kryxjs/core` is the foundation of the [Kryx](https://github.com/Brashkie)
multimedia ecosystem — the buffers, pipeline, sync, and I/O primitives every
other package (`ogg`, `wav`, `codecs`, `codecs-opus`, ...) builds on.

As the ecosystem grows, this roadmap is organized around **phases of evolution**
rather than version numbers. Versions are a consequence of releases; the phase
is what the work is actually building toward. Each shipped item notes the version
it landed in.

**Legend:** ✅ Complete · 🟡 Partial · 🔴 Planned · 🔵 Designed

---

## ✅ Phase 1 — Buffer foundation

The memory layer: zero-copy media buffers and an allocation pool that makes a
high-throughput pipeline cheap, all measured rather than assumed.

- [x] `MediaBuffer` — zero-copy frame buffer over `bytes::Bytes`, with frame
      metadata (pts/dts/duration/timebase/codec/flags) (v0.1.0)
- [x] `MediaBufferMut` — growable write buffer with builder (v0.1.0)
- [x] `MediaBuffer::clone` is zero-copy (shared allocation) (v0.1.0)
- [x] End-of-stream sentinel buffers (v0.1.0)
- [x] **`BufferPool`** — size-bucketed allocation recycling (4 KiB - 4 MiB),
      per-bucket caps, direct-allocation fallback for oversized frames, explicit
      `acquire` / `recycle` (v0.2.0)
- [x] **`PoolStats`** — hits / misses / recycled / dropped + `hit_rate()` (v0.2.0)
- [x] **Criterion benchmark suite** — pool vs. allocator; pool wins ~4x-830x
      because it skips the malloc + page-fault + free cycle (v0.2.0)
- [x] **Pool + `MediaBufferMut` integration** — `with_pool()`, `freeze()` (copy +
      recycle) and `freeze_zero_copy()` (share allocation) routes (v0.2.1)
- [x] **`SharedPool`** (`Rc<RefCell<BufferPool>>`) + `shared_pool()` for
      single-threaded, per-stage sharing (v0.2.1)
- [x] **End-to-end decoder-loop benchmark** — realistic frame sequence, ~99% hit
      rate; honest end-to-end numbers (freeze still copies) (v0.2.2)
- [x] **`PoolGuard` + `acquire_guard()`** — RAII wrapper that recycles on drop;
      `into_inner()` opts out (v0.2.3)
- [x] Pool types re-exported at the crate root (`kc_core::BufferPool`, ...) (v0.2.3)
- [x] **`reserve()`** — pre-warm the pool so a startup pipeline's first acquires
      are hits, not misses (v0.2.4)
- [x] **`memory_used()`** — bytes currently retained by the pool, for footprint
      observability (v0.2.4)
- [ ] 🔴 `PoolConfig` — configurable bucket range and caps per use case
- [ ] 🔴 `shrink_to()` — shed retained buffers under memory pressure

---

## 🟡 Phase 2 — Pipeline & execution

The processing model: stages composed into a pipeline, with the concurrency
story worked out.

- [x] `Stage` trait — async `process(MediaBuffer) -> Vec<MediaBuffer>` (v0.1.0)
- [x] `Pipeline` — ordered stage composition, pass-through, fan-out (v0.1.0)
- [x] Cancellation across stages (v0.1.0)
- [ ] 🔴 **Pool threading model** — decide thread-local vs `Send + Sync` pool for
      use inside a `Stage`, with benchmarks (the v0.2.2 finding: `SharedPool` is
      single-threaded and can't be a `Stage` field)
- [ ] 🔴 Producer/consumer between stages with bounded queues
- [ ] 🔴 Multi-source pipelines (mux several inputs)

---

## 🔴 Phase 3 — Observability

See what the pipeline is doing, at low cost — and the right place to settle the
pool's threading model with real numbers.

- [ ] 🔴 Pipeline metrics: per-stage throughput, allocation counts, drift
- [ ] 🔴 Optional `metrics` feature (via the `metrics` crate)
- [ ] 🔴 Tracing spans on every stage execution (via `tracing` feature)
- [ ] 🔴 Backpressure and queue-depth reporting between stages

---

## 🔵 Phase 4 — GPU primitives

Prepare for hardware acceleration. The largest jump in complexity, kept separate
on purpose.

- [ ] 🔵 `GpuBuffer` abstraction over Vulkan / Metal / D3D12 handles
- [ ] 🔵 CPU-GPU transfer helpers, zero-copy where the platform allows
- [ ] 🔵 Ownership, synchronization, and resource-lifetime model for GPU memory

---

## 🔴 Phase 5 — Real-time & streaming

Low-latency and network-aware operation.

- [ ] 🔴 `RealtimePipeline` variant with bounded queues + frame-drop policy
- [ ] 🔴 Jitter buffer primitive
- [ ] 🔴 Bitrate estimator and adaptive throttle hooks
- [ ] 🔴 RTP / WebRTC-friendly types

---

## 🔴 Phase 6 — Stability

- [ ] 🔴 API freeze and 1.0 — stable surface all Kryx packages build on
- [ ] 🔴 Plugin system for third-party stages
- [ ] 🔴 Full semver guarantees

---

## How we prioritize

1. **Measure, don't assume.** Every performance claim is backed by a benchmark;
   an optimization that doesn't show up in the numbers doesn't ship.
2. **Small scope, grow by proven need.** A primitive lands when a real consumer
   needs it — not before.
3. **Own the architecture, not every algorithm.** Kryx owns how the pieces fit;
   it reaches for the best algorithm in the parts that matter.
4. **Zero runtime dependencies** beyond the vetted core set.

*This roadmap is a direction, not a contract. Items move by proven need.*
