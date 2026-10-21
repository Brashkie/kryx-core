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
- [x] **`shrink_to()` / `shrink_to_bytes()`** — shed retained buffers under
      memory pressure (per-bucket cap or byte budget), the companion to
      `memory_used()` (v0.2.5)
- [ ] 🔴 `PoolConfig` — configurable bucket range and caps per use case

---

## ✅ Phase 2 — Pipeline & execution

The processing model: stages composed into a pipeline, with the concurrency
story worked out and a streaming executor.

- [x] `Stage` trait — async `process(MediaBuffer) -> Vec<MediaBuffer>` (v0.1.0)
- [x] `Pipeline` — ordered stage composition, pass-through, fan-out (v0.1.0)
- [x] Cancellation across stages (v0.1.0)
- [x] **Pool threading benchmark** — `arc_mutex` vs `thread_local` vs baseline
      under concurrent load, so the model was chosen with numbers (v0.2.5).
- [x] **Pool threading model — `WorkerPool`** — the benchmark's verdict, adopted:
      a `Send + Sync` per-thread pooling handle (`thread_local!` under the hood),
      no lock, no sharing. The private-pool-per-worker strategy won at every frame
      size; the shared `Arc<Mutex>` lost to no pooling at all on small frames.
      Resolves the v0.2.2 finding — the pool now works inside a `Send + Sync`
      `Stage` without being made lock-guarded (v0.2.6).
- [x] **Generic backpressure hook** — `LoadMetrics` + `BackpressurePolicy` +
      `BackpressureDecision`, with `Stage::load()`. Core reports saturation; the
      policy (in a higher layer) decides what it means. The mechanism, never the
      policy (v0.2.6).
- [x] **Streaming executor — `StreamPipeline`** — every stage runs as its own
      task over **bounded** channels (producer/consumer), with real backpressure
      driving the `BackpressurePolicy` and metrics fed live. True pipeline
      parallelism vs. the sequential `process` (v0.3.0)
- [x] **Multi-source** — `run_to_vec` muxes several input streams into the first
      stage via concurrent producers (v0.3.0)

---

## ✅ Phase 3 — Observability

See what the pipeline is doing, at low cost. Core counts; the caller interprets.

- [x] **Per-stage metrics** — `StageMetrics` (lock-free atomics): frames in/out/
      dropped, bytes, latency, filled automatically inside `process`. Snapshotted
      via `Pipeline::metrics()` → `PipelineMetrics` (v0.2.7)
- [x] **Backpressure/queue-depth reporting** — `StageMetrics::to_load_metrics()`
      turns real counters into the generic `LoadMetrics` a `BackpressurePolicy`
      reads, closing the loop with the 0.2.6 hook (v0.2.7)
- [x] **Tracing spans** — `trace-spans` feature wraps each stage execution in a
      `tracing` span via `.instrument()`; off by default, zero cost (v0.2.7)
- [x] **`MetricsSink` exporter hook** — `PipelineMetrics::export()` walks stages
      into a caller-supplied sink (Prometheus/StatsD/logs). Core emits, never
      depends on an exporter (v0.4.0)
- [x] **Drift/jitter tracking** — per-stage latency min/max → `jitter_ms()`,
      `min_latency_ms()`, `max_latency_ms()` on the snapshot (v0.4.0)

---

## 🟡 Phase 4 — GPU primitives

Prepare for hardware acceleration. The largest jump in complexity, kept separate
on purpose. Core owns the *architecture* of GPU memory, not any one graphics API.

- [x] **`GpuBuffer` + `GpuBackend` trait** — backend-agnostic opaque device
      buffer, RAII-freed; alloc/upload/download/free contract a real backend
      implements. No graphics deps in core (v0.4.0)
- [x] **`CpuFallbackBackend`** — host-memory implementation, so the abstraction
      runs and is testable today without any GPU (v0.4.0)
- [ ] 🔵 Real backends behind features: Vulkan / Metal / D3D12
- [ ] 🔵 Zero-copy CPU↔GPU paths (mapped/shared memory) where the platform allows
- [ ] 🔵 Synchronization + resource-lifetime model for cross-queue GPU work

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
