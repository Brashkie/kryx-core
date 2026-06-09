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

## v0.2.0 — Performance & GPU primitives

🎯 **Target: Q2 2025**

Focus: make the foundation faster and prepare for hardware acceleration.

**Planned:**
- [ ] `GpuBuffer` abstraction (Vulkan / Metal / D3D12 handles)
- [ ] CPU↔GPU transfer helpers with zero-copy when possible
- [ ] `BufferPool` for reusing allocations under pressure
- [ ] Pipeline metrics: per-stage throughput, allocation counts, drift
- [ ] Optional `metrics` feature using the `metrics` crate
- [ ] Tracing spans on every stage execution (via `tracing` feature)
- [ ] Benchmark suite with Criterion + flamegraphs in CI

---

## v0.3.0 — Real-time & streaming primitives

🎯 **Target: Q3 2025**

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
