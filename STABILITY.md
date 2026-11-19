# API Stability — `@kryxjs/core` (kc-core)

From **1.0.0**, `kc-core` follows [Semantic Versioning](https://semver.org).
This document states what is covered by that guarantee and what is not, so you
can depend on `1.x` with confidence.

## What SemVer covers (the stable surface)

Everything re-exported from the crate root (`kc_core::*`) is public API and will
not break within `1.x`:

- **Buffers** — `MediaBuffer`, `MediaBufferMut`, `MediaBufferBuilder`,
  `BufferPool`, `SharedPool`, `WorkerPool`, `PoolGuard`, `PoolStats`,
  `shared_pool`, `acquire_guard`, and the GPU surface (`GpuBuffer`,
  `GpuBackend`, `CpuFallbackBackend`, `best_backend`, `GpuError`, `WgpuBackend`).
- **Pipeline** — `Stage`, `StageContext`, `Pipeline`(+`Builder`),
  `StreamPipeline`(+`Builder`), `StageRegistry`, `StageConfig`, `ConfigValue`.
- **Backpressure & metrics** — `LoadMetrics`, `BackpressurePolicy`,
  `BackpressureDecision`, `NoBackpressure`, `StageMetrics`,
  `StageMetricsSnapshot`, `PipelineMetrics`, `MetricsSink`.
- **Real-time** — `JitterBuffer`, `JitterPush`, `BitrateEstimator`,
  `RealtimePolicy`.
- **Types & errors** — `MediaType`, `Timestamp`, and the `error` enums
  (`MediaError`, `Result`, …).

For these, within `1.x` we will not: remove or rename public items, change a
function signature incompatibly, or remove an enum variant / struct field.

## What is NOT covered

- **Anything `pub(crate)` or private** — internal implementation detail, free to
  change at any time (e.g. the `StageMetrics` recording methods, which only the
  pipeline calls).
- **Items behind a cargo feature** (e.g. `gpu-wgpu`, `trace-spans`) — the
  feature's surface may evolve faster; breaking changes there are documented but
  not held to the same bar.
- **Exact numeric behaviour** of metrics, timing, jitter and backpressure
  heuristics — the *shape* of the API is stable; tuning values may change.
- **Private struct fields** and anything not reachable from safe stable code.

## `#[non_exhaustive]` types

These may gain variants/fields in a minor `1.x` release, so match them with a
`_ => …` arm and construct them through their constructors/builders:

- Errors: `MediaError`, `BufferError`, `PipelineError`, `SyncError`, `GpuError`.
- Metadata / identity: `FrameMeta`, `ClockSource`, `ZigResult`.
- Value/format enums: `MediaType`, `CodecId`, `PixelFormat`, `SampleFormat`.

Types deliberately left **exhaustive** (safe to match without a wildcard, and
adding a variant would be a breaking `2.0`): `ConfigValue`, `JitterPush`,
`BackpressureDecision`, `SeekMode`.

## Minimum Supported Rust Version (MSRV)

`1.80`. Raising the MSRV is a **minor** version bump, not a patch, and will be
called out in the changelog.

## Deprecation policy

A public item is first marked `#[deprecated]` with a pointer to its replacement,
kept for the rest of the `1.x` series, and only removed in `2.0`.
