# Roadmap

This roadmap is **directional**, not contractual. Dates are intentions, not promises. Versions follow [Semantic Versioning](https://semver.org).

---

## Current: v0.1.x — Foundations

✅ **Released** — January 2025

The minimum viable core: buffers, pipelines, errors, timing, FFI bridge, full TypeScript layer with ESM + CJS dual-package.

**Shipped:**
- `MediaBuffer` with zero-copy semantics
- `Pipeline` + `Stage` trait (Rust)
- `Pipeline` + `Stage` interface (TypeScript)
- `MasterClock` + `StreamClock` for A/V sync
- `MediaSource` / `MediaSink` trait abstractions
- `MediaError` hierarchy with cause chains
- `Timebase` rational arithmetic with BigInt precision
- AbortSignal-aware pipelines
- Native addon stub for testing
- Dual CJS + ESM build via `tsup`
- Cross-platform CI (Windows / macOS / Linux × x64 / arm64)
- 100+ tests, ≥95% coverage

---

## v0.2.0 — Performance ✅ Shipped

Focus: make allocation cheap, and *prove* it with numbers.

**Done:**
- [x] `BufferPool` for reusing allocations under pressure — size-bucketed
      (4 KiB → 4 MiB), per-bucket caps, direct-allocation fallback for oversized
      frames, explicit `acquire` / `recycle`.
- [x] `PoolStats` — hit/miss/recycle/drop counters + `hit_rate()`.
- [x] Benchmark suite with Criterion. Result: the pool wins across every shape
      measured (~4×–830×), because it skips the malloc + page-fault + free cycle
      by reusing warm allocations.

## v0.2.1 — BufferPool integration ✅ Shipped

Focus: wire the pool into the write path without breaking any API.

**Done:**
- [x] `MediaBufferMut::with_pool(...)` — draw scratch from a `BufferPool`.
- [x] `MediaBufferMut::freeze()` recycles its scratch back to the pool.
- [x] `MediaBufferMut::freeze_zero_copy()` — freeze with no copy (share the
      allocation), for long-lived data.
- [x] `SharedPool` (`Rc<RefCell<BufferPool>>`) + `shared_pool()` for
      single-threaded, per-stage sharing.

## v0.2.2 — End-to-end pool validation ✅ Shipped

Focus: measure the pool in a realistic decoder loop, not in isolation.

**Done:**
- [x] `benches/decoder_loop.rs` — pooled vs. poolless `MediaBufferMut` over a run
      of frames, plus a steady-state hit-rate report from `PoolStats`.
- [x] Confirmed ~99% hit rate for a steady frame size; the end-to-end win is
      smaller than the micro-bench because `freeze()` still copies scratch → owned
      `Bytes` (honest decoder-shaped number).

**Architectural finding (input to v0.3):** `SharedPool` is `Rc<RefCell<_>>`
(single-threaded) and cannot be a field of a `Stage` (`Send + Sync`). The pool
was driven synchronously to model one decoder worker. The threading model — a
thread-local pool, a `Send + Sync` pool, or leaving it as-is — is a decision to
make with benchmarks in v0.3, not preemptively.

---

## v0.3.0 — Observability

🎯 **Target: Q3 2026**

Focus: see what the pipeline is doing, at low cost. Also the right place to
resolve the pool's threading model (per the v0.2.2 finding) with measurements.

**Planned:**
- [ ] Pipeline metrics: per-stage throughput, allocation counts, drift
- [ ] Optional `metrics` feature using the `metrics` crate
- [ ] Tracing spans on every stage execution (via `tracing` feature)
- [ ] Backpressure and queue-depth reporting between stages
- [ ] Decide pool threading model with benchmarks (thread-local vs Send+Sync)

---

## v0.4.0 — GPU primitives

🎯 **Target: Q4 2026**

Focus: prepare for hardware acceleration. The largest jump in complexity, kept
in its own version on purpose.

**Planned:**
- [ ] `GpuBuffer` abstraction (Vulkan / Metal / D3D12 handles)
- [ ] CPU↔GPU transfer helpers with zero-copy when possible
- [ ] Ownership, synchronization, and resource-lifetime model for GPU memory

---

## v0.5.0 — Real-time & streaming primitives

🎯 **Target: 2027**

Focus: low-latency and network-aware operation.

**Planned:**
- [ ] `RealtimePipeline` variant with bounded queues and frame-dropping policy
- [ ] Jitter buffer primitive
- [ ] Bitrate estimator and adaptive throttle hooks
- [ ] WebRTC-compatible types (RTP timestamps, NTP sync)
- [ ] Network clock variant (`MasterClock::network`)
- [ ] Backpressure signals from sinks → sources

---

## v0.4.0 — Plugin system

🎯 **Target: Q4 2025**

Focus: enable third-party stages without forking.

**Planned:**
- [ ] Plugin manifest spec (`media.json` next to native binaries)
- [ ] Dynamic loading via `libloading`
- [ ] Plugin registry pattern + discovery
- [ ] Stable plugin ABI (versioned, opt-in)
- [ ] CLI: `kryxjs plugins list / install / verify`

---

## v0.5.0 — Producer/consumer & multi-source

🎯 **Target: Q1 2026**

Focus: complex topologies beyond linear chains.

**Planned:**
- [ ] Multi-source merging (audio + video synchronized at the source)
- [ ] Split/tee stage (one in, N out independently)
- [ ] Linear pipeline groups (parallel branches converging back)
- [ ] Pull-based pipelines (driven by sink rather than source)
- [ ] Time-aware joining of streams

---

## v1.0.0 — Stable API

🎯 **Target: Q2 2026**

Focus: lock the API, ship LTS guarantees.

**Requirements before 1.0:**
- ✅ Every type stable for two minor versions
- ✅ All `@kryxjs/*` packages built on top
- ✅ Documentation complete (every public symbol)
- ✅ Migration guide from 0.x
- ✅ At least one third-party plugin in the wild
- ✅ Production deployments on record

After 1.0:
- Strict semver (no breaking changes within a major)
- 12-month support window per major
- LTS releases tagged

---

## Beyond 1.0

**v1.x** — Iterative improvements within stable API:
- More codecs supported in `media-codecs`
- More container formats in `media-containers`
- Web platform target (WASM)
- Embedded target (no_std)

**v2.0** — Distant future. Only if breaking changes are demonstrably worth it (e.g. a major Rust language feature changes what's possible).

---

## How we prioritize

1. **Unblocking downstream packages.** If `@kryxjs/codecs` needs something, that something jumps the queue.
2. **Stability over features.** A small API that works perfectly beats a big one with footguns.
3. **Performance is a feature.** Regressions block releases.
4. **Cross-platform is non-negotiable.** Anything that doesn't work on all supported targets doesn't ship.

---

## Want to influence the roadmap?

- [Open an issue](https://github.com/Brashkie/kryx-core/issues) describing your use case
- [Start a discussion](https://github.com/Brashkie/kryx-core/discussions) for design ideas
- Submit a PR with tests for your scenario

Roadmap items with concrete user demand jump the queue. Theoretical features wait.
