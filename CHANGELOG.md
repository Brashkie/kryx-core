# Changelog

All notable changes to `@kryxjs/core` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.2.4] — 2026-08-14

**Phase 1 hardening + roadmap restructure.** Strengthens the buffer-pool
foundation with pre-warming and memory observability, and reorganizes the roadmap
around phases instead of version numbers.

### Added
- `BufferPool::reserve(size, count)` — pre-warm the pool by allocating `count`
  buffers sized for `size`, so a pipeline's first acquires are hits instead of
  misses (pay the allocation + page-fault cost at startup, not on the first
  frames). Respects the per-bucket cap; ignores sizes above the largest bucket.
- `BufferPool::memory_used()` — total bytes currently retained by pooled buffers,
  for footprint observability (distinct from `pooled_count()` and the hit-rate
  stats).

### Changed
- **Roadmap restructured into phases.** `docs/ROADMAP.md` now describes phases of
  evolution (Buffer foundation, Pipeline & execution, Observability, GPU, Real-time,
  Stability) with each shipped item annotated by the version it landed in, rather
  than a version-by-version list. Versions are a consequence of releases; the phase
  is what the work builds toward. Also removed duplicate v0.4/v0.5 sections the old
  structure had accumulated.

### Notes
- Phase 1 (Buffer foundation) is now feature-complete for the pool's core: still
  open are `PoolConfig` (configurable buckets) and `shrink_to()` (shed memory
  under pressure), both marked planned.

## [0.2.3] — 2026-08-13

**API polish — closes the 0.2.x series.** No new mechanics; makes the buffer-pool
surface pleasant to use and adds the ergonomic RAII layer deferred in 0.2.0.

### Added
- `PoolGuard` + `acquire_guard(pool, size)` — an RAII wrapper that derefs to the
  underlying `BytesMut` and recycles itself on drop, so scope-based use never
  forgets to recycle. `PoolGuard::into_inner()` opts out (takes ownership, no
  recycle) when the buffer must outlive the guard. This is the layer over the
  explicit `acquire` / `recycle` primitive, promised in 0.2.0.

### Changed
- Pool types are now re-exported at the crate root: `kc_core::BufferPool`,
  `PoolStats`, `SharedPool`, `shared_pool`, `PoolGuard`, `acquire_guard` — no
  need to reach into `kc_core::buffer::*`. The explicit primitive and the
  `MediaBufferMut` freeze routes are unchanged.

### The 0.2.x series
- **0.2.0** — `BufferPool` + Criterion benchmarks (pool wins ~4×–830×).
- **0.2.1** — pool integrated into `MediaBufferMut` (copy+recycle and zero-copy
  freeze routes).
- **0.2.2** — end-to-end decoder-loop validation (~99% hit rate; found that
  `SharedPool` is single-threaded, an input to v0.3's threading decision).
- **0.2.3** — API polish + RAII guard (this release).

## [0.2.2] — 2026-08-12

**End-to-end pool validation.** Adds a decoder-shaped benchmark that measures the
`BufferPool` the way a real decoder uses it — a sequence of frames, each built,
written, and `freeze()`d — rather than in isolation.

### Added
- `benches/decoder_loop.rs` — Criterion benchmark comparing pooled vs. poolless
  `MediaBufferMut` over a run of frames at several sizes, and a `hit_rate` report
  showing the pool's steady-state reuse fraction via `PoolStats`.

### Notes
- Confirms the pool reaches ~99% hit rate for a steady frame size (one initial
  miss, then reuse). The end-to-end win is smaller than the isolated micro-bench
  because `freeze()` still copies scratch → owned `Bytes` — a real cost the pool
  does not remove. This is the honest, decoder-shaped number.
- **Architectural finding:** `SharedPool` is `Rc<RefCell<_>>` (single-threaded),
  so it cannot be a field of a `Stage` (which is `Send + Sync`). The pool is used
  synchronously here, modeling one decoder worker. Whether v0.3 needs a
  thread-local pool, a `Send + Sync` pool, or leaves this as-is is a decision to
  make with numbers, not preemptively — no architecture was changed to fit the
  benchmark.

## [0.2.1] — 2026-08-11

**`BufferPool` integration into `MediaBufferMut`.** The pool from 0.2.0 is now
wired into the growable write buffer, so decoders and stages can recycle their
scratch allocations. No existing API changes — the poolless path is untouched.

### Added
- `MediaBufferMut::with_pool(pool, media_type, capacity)` — draw the scratch
  allocation from a `BufferPool` instead of allocating fresh.
- `MediaBufferMut::freeze_zero_copy()` — freeze with **no copy**, converting the
  scratch directly into shared `Bytes` (the resulting `MediaBuffer` owns the
  allocation; nothing returns to the pool).
- `SharedPool` (`Rc<RefCell<BufferPool>>`) type alias + `shared_pool()` helper
  for single-threaded, per-stage pool sharing.

### Changed
- `MediaBufferMut::freeze()` now recycles its scratch back to the pool (when the
  buffer was created via `with_pool`), after copying the bytes out. Behavior for
  buffers created with `with_capacity` (no pool) is unchanged.

### Notes
- Two freeze routes let the caller pick by data lifetime: `freeze()` for
  reusable pipelines (copy + recycle, bounded memory), `freeze_zero_copy()` for
  long-lived data (share allocation, skip the copy).

## [0.2.0] — 2026-08-08

**Performance.** First measured optimization: a buffer pool that recycles
allocations instead of hitting the allocator on every frame.

### Added
- `BufferPool` — size-bucketed allocation recycling (power-of-two buckets from
  4 KiB to 4 MiB), explicit `acquire(size)` / `recycle(buf)`, per-bucket caps
  (default 32) so a spike of large frames can't pin memory indefinitely, and
  direct allocation (dropped on recycle) for anything above the largest bucket.
- `PoolStats` — hits, misses, recycled, dropped, and `hit_rate()`, so pool
  effectiveness can be measured in production rather than assumed.
- Criterion benchmark suite (`benches/buffer_pool.rs`) comparing the pool
  against direct allocation across frame sizes, tight churn, and mixed load.

### Performance
Measured with Criterion (`cargo bench`), pool vs. a fresh allocation per frame.
The pool wins across every shape tested — reusing an already-warm allocation
skips the malloc + page-fault + free cycle:

| Workload | Baseline | Pool | Speedup |
|----------|----------|------|---------|
| 4 KiB frame | 48.6 ns | 12.3 ns | ~4× |
| 64 KiB frame | 159.6 ns | 12.0 ns | ~13× |
| 1 MiB frame | 10.3 µs | 12.4 ns | ~830× |
| churn (256 × 64 KiB) | 68.5 µs | 2.62 µs | ~26× |
| mixed load | 1.19 µs | 82.8 ns | ~14× |

Figures are best-case (100% reuse); real-world gains depend on hit rate, which
`PoolStats.hit_rate()` reports.

## [0.1.0] — 2026-06-08

**First public release of `@kryxjs/core`.**

This package is the direct successor of [`@brashkie/media-core@0.1.4`](https://www.npmjs.com/package/@brashkie/media-core), rebranded under the new `@kryxjs/*` scope. The public TypeScript API is **identical**; only the package name, repository, and one binding-level fix change.

### Added
- Initial release under `@kryxjs/core`.
- `MediaBuffer` — zero-copy refcounted frame buffer (Rust + napi-rs).
- `Pipeline` + `PipelineBuilder` with async stages, fan-out, `AbortSignal`.
- Built-in stages: `PassthroughStage`, `DropStage`, `CounterStage`, `tapStage`.
- `Timebase` + `Timestamp` for rational timestamp arithmetic with BigInt rescale.
- `MediaError` hierarchy: `BufferError`, `PipelineError`, `IoError`, `FfiError`, `SyncError`.
- `MediaType`, `CodecId`, `PixelFormat`, `SampleFormat` discriminants.
- Dual ESM + CJS build via tsup with proper `.d.mts` / `.d.cts`.
- 7 platforms supported via per-platform npm sub-packages (`@kryxjs/core-<platform>`).
- CI on Windows / macOS / Linux (x64 + arm64, gnu + musl).

### Changed (vs `@brashkie/media-core@0.1.4`)
- **Naming**: `@brashkie/media-core` → `@kryxjs/core`.
- **Repository**: now lives at <https://github.com/Brashkie/kryx-core>.
- **Rust crate names**: `mc-core` → `kc-core`, `mc-node` → `kc-node`.

### Fixed
- **Buffer types in napi binding** — `MediaBuffer.video()` and `MediaBuffer.audio()` now accept `Buffer | Uint8Array` directly, using `napi::bindgen_prelude::Buffer` in the Rust binding. Previously required `Array<number>` due to a binding bug that forced ugly casts like `[1, 2, 3] as unknown as Buffer`. The `data()` method now also returns `Buffer` directly.

### Migration
See [docs/MIGRATION.md](docs/MIGRATION.md).

---

## Pre-history — `@brashkie/media-core`

The following entries are from the predecessor package `@brashkie/media-core`. They are preserved here for continuity but apply to the **old** package name. Versions of `@kryxjs/core` start fresh at `0.1.0`.

---

## [0.1.4] — 2026-05-26

### Fixed
- **Critical for downstream consumers**: tsup's `shims: true` option rewrote
  the runtime `require('../index.js')` into a `__require()` helper that
  throws `'Dynamic require of "../index.js" is not supported'` whenever
  `require` is not available at runtime. This broke any consumer running
  inside Vitest, Vite, or other tools that load CJS via an ESM wrapper —
  including `@kryxjs/codecs` tests.
- Removed `shims: true` from `tsup.config.ts`.
- Rewrote `src/index.ts` to use `createRequire` from `node:module`
  explicitly, with a CJS/ESM dual-path that resolves the module base from
  either `__filename` or `import.meta.url`. Now works identically in
  Node CJS, Node ESM, and Vitest.

### Notes
- Consumers of `@kryxjs/core` should upgrade to `^0.1.4`.
- `0.1.3` is being deprecated on npm.

---

## [0.1.3] — 2026-05-25

### Fixed
- tsup build only emitted a single `.d.mts` file even though `outExtension`
  requested both `.d.cts` and `.d.mts`. Switched to a post-build script
  (`scripts/fix-dts.js`) that duplicates `index.d.ts` into the per-format
  variants required by the `exports` map.

### Known issues
- The `shims: true` config still broke downstream consumers (fixed in 0.1.4).

---

## [0.1.2] — 2026-05-25

### Fixed
- `tsup` DTS build failed because the auto-generated `index.d.ts` from napi
  wasn't recognized as a module. `src/index.ts` now declares the native
  addon's shape inline, decoupling the TypeScript layer from napi's codegen.
- Native addon loading: `index.js` placeholder now gracefully stubs when
  the `.node` binary is missing, throwing only on actual use.
- ESM build: enabled `tsup` `shims: true` so the runtime `require()` of the
  native addon works in both CJS and ESM outputs. **NOTE**: this turned out
  to be incorrect — fixed in 0.1.4.

### Changed
- `MediaBuffer` is now exported as both a value (constructor) and a type alias.

---

## [0.1.1] — 2026-05-24

### Fixed
- CI/build: migrated from `@napi-rs/cli` v2 to v3 syntax — but later reverted
  in 0.1.2 because v3 requires Node 20+, breaking our Alpine CI on Node 18.
- `scripts/create-npm-dirs.js` now also copies the `.node` binaries from
  CI artifacts into each per-platform npm package directory.

---

## [0.1.0] — 2026-05-24

**First public release.** Foundation layer of the Kryx ecosystem.

### Added
- `MediaBuffer` — zero-copy refcounted frame buffer with metadata
- `Pipeline` + `PipelineBuilder` with async stages, fan-out, AbortSignal
- Built-in stages: `PassthroughStage`, `DropStage`, `CounterStage`, `tapStage`
- `Timebase` + `Timestamp` for exact rational timestamp arithmetic
- `MediaType`, `CodecId`, `PixelFormat`, `SampleFormat` discriminants
- `MediaError` hierarchy with `kind`, `context`, `details`, ES2022 `cause`
- `MasterClock` + `StreamClock` for A/V synchronization
- `MediaSource` + `MediaSink` async I/O traits
- FFI bridge for Zig modules (feature-gated)
- Dual ESM + CJS build via `tsup` with proper `.d.mts` / `.d.cts`
- TypeScript 6.0 strict mode
- 7 platforms supported via per-platform npm sub-packages
- Cross-platform CI
- 16 Rust tests + 85+ TypeScript tests
- ≥95% coverage target

---

[Unreleased]: https://github.com/Brashkie/kryx-core/compare/v0.1.4...HEAD
[0.1.4]: https://github.com/Brashkie/kryx-core/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/Brashkie/kryx-core/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/Brashkie/kryx-core/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/Brashkie/kryx-core/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/Brashkie/kryx-core/releases/tag/v0.1.0

