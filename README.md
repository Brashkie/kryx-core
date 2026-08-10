<div align="center">

<img src="media/logo.png" alt="Kryx core" width="280" />

**The foundational layer of the [Kryx](https://kryx.dev) multimedia ecosystem**

Zero-copy media buffers · Async pipelines · A/V sync · Rust + napi-rs

[![CI](https://github.com/Brashkie/kryx-core/actions/workflows/ci.yml/badge.svg)](https://github.com/Brashkie/kryx-core/actions)
[![npm version](https://img.shields.io/npm/v/@kryxjs/core?color=cb3837&logo=npm)](https://npmjs.com/package/@kryxjs/core)
[![npm downloads](https://img.shields.io/npm/dm/@kryxjs/core?color=cb3837&logo=npm)](https://npmjs.com/package/@kryxjs/core)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue)](LICENSE)
[![rust 1.80+](https://img.shields.io/badge/rust-1.80%2B-orange?logo=rust)](https://www.rust-lang.org)
[![node ≥18](https://img.shields.io/badge/node-%E2%89%A518-3c873a?logo=node.js)](https://nodejs.org)
[![TypeScript](https://img.shields.io/badge/TypeScript-6.0-3178c6?logo=typescript)](https://www.typescriptlang.org/)
[![coverage](https://img.shields.io/badge/coverage-%E2%89%A595%25-brightgreen)](#-testing)

**English** · [Español](README.es.md) · [API Docs](https://docs.kryx.dev/core) · [Roadmap](docs/ROADMAP.md) · [Architecture](docs/ARCHITECTURE.md) · [Changelog](CHANGELOG.md)

</div>

---

## What is this?

`@kryxjs/core` is the foundational layer of [Kryx](https://kryx.dev) — a modern multimedia ecosystem for Node.js. Think `libavutil`, but built around composable modules, native async, and Rust's memory safety.

It does **not** include codecs, containers, or streaming protocols. Those live in dedicated `@kryxjs/*` packages. This package gives you the primitives every multimedia tool needs: buffers, pipelines, timestamps, error types, and the FFI bridge to Zig.

```bash
npm install @kryxjs/core
```

```ts
import { MediaBuffer, Pipeline, PassthroughStage } from '@kryxjs/core'

const pipeline = Pipeline.builder()
  .stage(PassthroughStage)
  .build()

const buf = MediaBuffer.video(rawFrame, /* pts */ 90_000)
const [out] = await pipeline.process(buf)

console.log(out.pts, out.mediaType, out.codecId)
// → 90000 video h264
```

---

## Why?

| | |
|---|---|
| 🎯 **Zero-copy buffers** | Frames are refcounted `Bytes` — clones are free, slicing doesn't allocate |
| ⚡ **Native performance** | Hot paths in Rust; codecs and SIMD in Zig (downstream packages) |
| 🔌 **Async-first** | Built on `tokio` + ES2022 — backpressure, AbortSignal, async iterables |
| 🧩 **Composable** | Stages snap together. No monolith. Each package does one thing |
| 🔒 **Type-safe** | TypeScript 6.0 strict mode + auto-generated `.d.ts` from napi-rs |
| 📦 **Dual-package** | First-class ESM and CJS — pick whatever your project uses |
| 🌐 **Cross-platform** | Windows, macOS, Linux — x64 and arm64, glibc and musl |
| 🧪 **Tested** | 16 Rust tests + 85+ TS tests + dual-package smoke. ≥95% coverage |
| 🪶 **Small** | < 80 KB unpacked (without the native `.node`, which is platform-specific) |

---

## The Kryx ecosystem

`@kryxjs/core` is the base. Every other package builds on top.

```
                          ┌─────────────────┐
                          │      kryx       │  ← unified SDK (the "FFmpeg" facade)
                          └────────┬────────┘
                                   │
        ┌──────────────────────────┼──────────────────────────┐
        ▼                          ▼                          ▼
┌────────────────┐  ┌──────────────────────┐  ┌─────────────────────┐
│ media-video    │  │ media-audio          │  │ media-subtitles     │
│ media-codecs   │  │ media-ai             │  │ media-stream        │
│ media-gpu      │  │ media-containers     │  │ media-cli           │
└────────┬───────┘  └──────────┬───────────┘  └──────────┬──────────┘
         │                     │                          │
         └─────────────────────┼──────────────────────────┘
                               ▼
                    ┌──────────────────────┐
                    │  @kryxjs/core │   ← you are here
                    │   (Rust + napi-rs)    │
                    └──────────┬───────────┘
                               │
                               ▼
                    ┌──────────────────────┐
                    │   Zig low-level       │
                    │   (codecs, SIMD, GPU) │
                    └──────────────────────┘
```

---

## Installation

```bash
npm install @kryxjs/core
# or
pnpm add @kryxjs/core
# or
yarn add @kryxjs/core
```

Prebuilt binaries ship for all supported platforms via npm `optionalDependencies`. **No Rust toolchain required** to install — only to contribute.

| OS | Architecture | npm sub-package |
|----|--------------|-----------------|
| 🪟 Windows | x64 | `@kryxjs/core-win32-x64-msvc` |
| 🪟 Windows | arm64 | `@kryxjs/core-win32-arm64-msvc` |
| 🍎 macOS | x64 (Intel) | `@kryxjs/core-darwin-x64` |
| 🍎 macOS | arm64 (Apple Silicon) | `@kryxjs/core-darwin-arm64` |
| 🐧 Linux | x64 (glibc) | `@kryxjs/core-linux-x64-gnu` |
| 🐧 Linux | x64 (musl/Alpine) | `@kryxjs/core-linux-x64-musl` |
| 🐧 Linux | arm64 (glibc) | `@kryxjs/core-linux-arm64-gnu` |

---

## Quick start

### 1. Pure pass-through pipeline

```ts
import { MediaBuffer, Pipeline, PassthroughStage } from '@kryxjs/core'

const pipeline = Pipeline.builder()
  .name('passthrough')
  .stage(PassthroughStage)
  .build()

const buf = MediaBuffer.video(Buffer.from([0x00, 0x00, 0x00, 0x01]), 90_000)
const [out] = await pipeline.process(buf)

console.log(out === buf) // true — zero-copy
```

### 2. Custom stage (filter)

```ts
import { Pipeline, type Stage, type MediaFrameLike } from '@kryxjs/core'

const dropEosStage: Stage = {
  name: 'drop-eos',
  process(frame) {
    return frame.isEos ? [] : [frame]
  },
}

const pipeline = Pipeline.builder()
  .stage(dropEosStage)
  .build()

await pipeline.process({ pts: 0 })          // → [{ pts: 0 }]
await pipeline.process({ pts: 1, isEos: true }) // → []  (dropped)
```

### 3. Multi-stage with tap (logging)

```ts
import {
  MediaBuffer, Pipeline, PassthroughStage,
  CounterStage, tapStage,
} from '@kryxjs/core'

const counter = new CounterStage()

const pipeline = Pipeline.builder()
  .name('demo')
  .stage(PassthroughStage)
  .stage(tapStage('log', (frame, ctx) => {
    console.log(`#${ctx.frameCount} pts=${frame.pts} type=${frame.mediaType}`)
  }))
  .stage(counter)
  .build()

await pipeline.start()
for (let i = 0; i < 100; i++) {
  await pipeline.process(MediaBuffer.video(payload, i * 3_000))
}
console.log(`Total: ${counter.count} frames`)
await pipeline.stop()
```

### 4. Fan-out (one frame → many)

```ts
const duplicator: Stage = {
  name: 'duplicate',
  process(frame) {
    return [frame, { ...frame, pts: frame.pts! + 1 }]
  },
}

const pipeline = Pipeline.builder().stage(duplicator).build()
const out = await pipeline.process({ pts: 100 })
// out.length === 2, pts values: [100, 101]
```

### 5. Async iterable (streaming)

```ts
async function* readFrames(): AsyncIterable<MediaBuffer> {
  while (hasMore()) yield await readNextFrame()
}

for await (const frame of pipeline.processStream(readFrames())) {
  await sink.write(frame)
}
```

### 6. Cancellation with AbortSignal

```ts
const ac = new AbortController()

const pipeline = Pipeline.builder()
  .stage(slowStage)
  .signal(ac.signal)
  .build()

setTimeout(() => ac.abort('user cancelled'), 5_000)

try {
  await pipeline.process(buf)
} catch (err) {
  if (MediaError.isKind(err, 'timeout')) {
    console.log('Cancelled:', err.cause)
  }
}
```

### 7. Timebase math (exact, no precision loss)

```ts
import { Timebase } from '@kryxjs/core'

// Convert from 90kHz (MPEG video) to milliseconds
const ms = Timebase.VIDEO_90K.rescale(90_000, Timebase.MILLISECOND)
console.log(ms) // 1000

// Custom timebases — e.g. 25 fps
const fps25 = Timebase.of(1, 25)
fps25.toSeconds(100) // 4 — 100 frames at 25fps = 4 seconds

// Long-running PTS values use BigInt internally — no float drift
Timebase.AUDIO_48K.rescale(1_000_000_000, Timebase.MILLISECOND)
// → 20833333 (precise integer, no rounding error)
```

---

## Core concepts

### `MediaBuffer` — the unit of data

Wraps a refcounted byte buffer with metadata: codec, PTS/DTS, frame flags, stream index, optional per-type metadata (video resolution / audio sample rate).

```ts
const frame = MediaBuffer.video(data, /* pts */ 90_000)

frame.pts          // 90000
frame.codecId      // 'h264'
frame.mediaType    // 'video'
frame.len          // bytes
frame.isEmpty      // false
frame.isKeyframe   // false
frame.isEos        // false
frame.data()       // Buffer — zero-copy view
```

**Cloning is free** — only refcount bumps. No `memcpy`.

### `Stage` — a processing unit

Each stage receives one frame and returns zero or more frames:

```ts
interface Stage<TFrame = MediaFrameLike> {
  readonly name: string
  process(frame: TFrame, ctx: StageContext): Promise<readonly TFrame[]> | readonly TFrame[]
  onStart?(ctx: { pipelineName: string; signal?: AbortSignal }): Promise<void> | void
  onStop?(): Promise<void> | void
  readonly accepts?: readonly MediaType[]
}
```

| Return value | Behavior |
|--------------|----------|
| `[frame]` | Pass through |
| `[]` | Drop the frame |
| `[a, b, c]` | Fan-out into multiple frames |
| `Promise<...>` | Async stages are supported natively |

### `Pipeline` — composition

```ts
const pipeline = Pipeline.builder()
  .name('transcode')                                  // optional
  .stage(decoderStage)
  .stage(resizerStage)
  .stage(encoderStage)
  .signal(abortController.signal)                     // optional
  .onError((err, stageName) => log.error(stageName))  // optional
  .build()

// Lifecycle (optional but recommended for stages with resources)
await pipeline.start()

// Process single, batch, or stream
const out      = await pipeline.process(frame)
const batch    = await pipeline.processBatch([f1, f2, f3])
for await (const f of pipeline.processStream(source)) { /* ... */ }

await pipeline.stop()
```

### Error hierarchy

All errors derive from `MediaError` and carry a discriminant `kind`:

| `kind` | Class | Used for |
|--------|-------|----------|
| `'buffer'` | `BufferError` | Invalid buffer state, OOB access |
| `'pipeline'` | `PipelineError` | Stage failures, invalid composition |
| `'io'` | `IoError` | Source/sink errors |
| `'ffi'` | `FfiError` | Native addon / Zig bridge errors |
| `'sync'` | `SyncError` | A/V sync drift exceeded threshold |
| `'unsupported'` | — | Format/codec not implemented |
| `'invalid_timestamp'` | — | PTS validation failed |
| `'closed'` | — | Operation on a closed resource |
| `'timeout'` | — | AbortSignal triggered or deadline exceeded |
| `'internal'` | — | Bug — should never happen |

```ts
try {
  await pipeline.process(buf)
} catch (err) {
  if (MediaError.isKind(err, 'pipeline')) {
    console.error('Stage failed:', err.context, err.cause)
  }
  if (err.isRecoverable) retry()
  if (err.isFatal)       crash()
}
```

Errors are JSON-serializable via `toJSON()` and carry ES2022 `cause` chains.

---

## Comparison

How does this compare to existing tools?

| Feature | `@kryxjs/core` | FFmpeg (C) | GStreamer | beamcoder / ffmpeg-static |
|---|---|---|---|---|
| **Language** | Rust + TypeScript | C | C | C bindings |
| **Memory safety** | ✅ Guaranteed | ⚠️ Manual | ⚠️ Manual | ⚠️ Inherited from C |
| **Async-native** | ✅ `async`/`await` | ❌ Callbacks/threads | ⚠️ GLib mainloop | ⚠️ Callbacks |
| **Modular** | ✅ Per-feature npm packages | ❌ Monolith | ✅ Plugin system | ❌ Monolith |
| **TypeScript** | ✅ First-class | ❌ | ❌ | ⚠️ Community types |
| **Zero-copy buffers** | ✅ Refcounted `Bytes` | ✅ `AVBufferRef` | ✅ `GstBuffer` | ⚠️ Often copies |
| **AbortSignal support** | ✅ | ❌ | ❌ | ❌ |
| **Prebuilt binaries** | ✅ 7 platforms | ❌ | ❌ | ✅ |
| **Install size** | ~3 MB (per platform) | ~70 MB | ~60 MB | ~70 MB |
| **Scope** | 📦 Primitives only | 🎬 Full transcoding | 🎬 Full pipeline | 🎬 Full transcoding |

**`@kryxjs/core` is not a replacement for FFmpeg today** — it's the foundation Kryx is building toward an equivalent set of capabilities, modularly, in Rust+Zig.

---

## Performance characteristics

Approximate numbers from local benchmarks (Ryzen 7 5800X, Node 20):

| Operation | Time | Notes |
|---|---|---|
| `MediaBuffer.video()` construction | ~150 ns | Just a struct + Arc bump |
| `MediaBuffer.clone()` | ~50 ns | Refcount only — no copy |
| `Pipeline.process()` (10-stage passthrough) | ~3 µs | Mostly JS function call overhead |
| `Timebase.rescale()` | ~200 ns | BigInt path on large values |
| Native ↔ JS boundary cross | ~400 ns | napi-rs typed function calls |

Real workloads are dominated by codec/IO, not these primitives. Bench yours.

### `BufferPool` (0.2.0)

In a pipeline that processes thousands of frames per second, allocating and
freeing a buffer for every frame keeps the allocator busy. `BufferPool` recycles
those allocations instead — reusing an already-warm buffer skips the
malloc + page-fault + free cycle entirely.

```rust
use kc_core::buffer::BufferPool;

let mut pool = BufferPool::new();

let mut buf = pool.acquire(64 * 1024); // cleared, capacity ≥ 64 KiB
buf.extend_from_slice(frame_bytes);
// ... use the buffer ...
pool.recycle(buf); // back to the pool for the next frame

println!("hit rate: {:.1}%", pool.stats().hit_rate() * 100.0);
```

It is **size-bucketed** (power-of-two buckets, 4 KiB → 4 MiB), **capped per
bucket** so a spike of large frames can't pin memory indefinitely, and falls
back to direct allocation above the largest bucket. Measured with Criterion
(`cargo bench`), pool vs. a fresh allocation per frame:

| Workload | Baseline | Pool | Speedup |
|---|---|---|---|
| 4 KiB frame | 48.6 ns | 12.3 ns | ~4× |
| 64 KiB frame | 159.6 ns | 12.0 ns | ~13× |
| 1 MiB frame | 10.3 µs | 12.4 ns | ~830× |
| churn (256 × 64 KiB) | 68.5 µs | 2.62 µs | ~26× |
| mixed load | 1.19 µs | 82.8 ns | ~14× |

Figures are best-case (100% reuse); real gains depend on hit rate, which
`PoolStats` reports.

---

## Development

### Prerequisites

- [Rust ≥1.80](https://rustup.rs) — `rustup install stable`
- Node.js ≥18
- `@napi-rs/cli`: comes via `devDependencies`

### Local setup

```bash
git clone https://github.com/Brashkie/kryx-core.git
cd kryx-core
npm install
npm run build:debug   # builds Rust addon (debug) + TypeScript
npm test              # runs all tests
```

### Scripts

| Command | Description |
|---|---|
| `npm run build` | Production build (Rust release + TypeScript) |
| `npm run build:debug` | Debug build (faster compile, slower runtime) |
| `npm run build:native` | Rust addon only |
| `npm run build:ts` | TypeScript only |
| `npm test` | Rust + TS + dual-package smoke |
| `npm run test:vitest` | TS tests only |
| `npm run test:watch` | TS tests in watch mode |
| `npm run test:coverage` | Coverage report (v8 provider) |
| `npm run test:coverage:ui` | Interactive coverage UI |
| `npm run typecheck` | `tsc --noEmit` |
| `npm run lint` | ESLint |
| `npm run clippy` | Rust clippy |
| `npm run format` | Prettier |
| `npm run format:rust` | rustfmt |
| `npm run examples` | Run all `examples/*` files |
| `npm run clean` | Remove `dist/`, `target/`, `npm/`, `.node` |

### Project structure

```
kryx-core/
├── src/                    TypeScript source (public API)
│   ├── index.ts            Public entry point
│   ├── pipeline.ts         Pipeline + Stage trait
│   ├── types.ts            Timebase, Timestamp, MediaType, CodecId
│   └── error.ts            MediaError hierarchy
│
├── crates/
│   ├── kc-core/            Pure Rust core (no Node deps)
│   │   └── src/
│   │       ├── buffer/     Zero-copy MediaBuffer
│   │       ├── pipeline/   Pipeline + Stage trait
│   │       ├── sync/       MasterClock + StreamClock
│   │       ├── io/         MediaSource + MediaSink
│   │       ├── ffi/        Zig bridge (feature-gated)
│   │       ├── types/      Shared types
│   │       └── utils/
│   │
│   └── kc-node/            napi-rs bindings → .node binary
│       └── src/lib.rs
│
├── __tests__/              Vitest suite (100+ tests)
├── examples/               Runnable CJS/ESM/TS examples
├── scripts/                Build helpers (per-platform npm pkgs)
├── npm/                    Per-platform native packages (generated)
├── docs/                   ARCHITECTURE, ROADMAP, CONTRIBUTING, etc.
└── .github/workflows/      CI: test matrix + release pipeline
```

---

## Testing

```bash
npm test
```

Runs three suites:

1. **16 Rust tests** in `crates/kc-core` — covering buffer, pipeline, types, ffi, sync
2. **85+ TypeScript tests** with Vitest — covering error hierarchy, Timebase math (with BigInt overflow paths), Pipeline lifecycle, AbortSignal, fan-out, async stages, and mock native bindings
3. **Dual-package smoke tests** — verifying CJS and ESM imports both resolve correctly

Target coverage: **≥95%** across all files. Open `coverage/index.html` after `npm run test:coverage`.

---

## Contributing

PRs welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md) for:

- Code style (rustfmt + prettier, enforced)
- Conventional Commits
- PR checklist (tests + types + docs + changelog)

Out of scope for this package (belongs in `@kryxjs/*`):
- Codec implementations
- Container parsing
- Streaming protocols
- AI/ML models
- CLI tools

When in doubt, [open a discussion](https://github.com/Brashkie/kryx-core/discussions) first.

---

## Roadmap

See [docs/ROADMAP.md](docs/ROADMAP.md) for the full plan.

| Version | Focus | Target |
|---------|-------|--------|
| **0.1.x** | Foundations | ✅ Shipped |
| **0.2** | Performance — `BufferPool` + benchmarks | ✅ Shipped |
| **0.2.1** | `BufferPool` integration into `MediaBufferMut` | Next |
| **0.3** | Observability — pipeline metrics + tracing spans | Q3 2026 |
| **0.4** | GPU buffer abstractions + hw-accel primitives | Q4 2026 |
| **0.5** | Real-time / streaming (RTP, WebRTC types, jitter buffer) | 2027 |
| **1.0** | Stable API + all Kryx packages on top | 2027 |

---

## FAQ

<details>
<summary><b>Do I need Rust installed to use this?</b></summary>

No. Prebuilt binaries are shipped for all supported platforms via npm `optionalDependencies`. Just `npm install @kryxjs/core` and you're set.

You only need Rust if you're contributing or building from source.
</details>

<details>
<summary><b>Why two languages (Rust + Zig)?</b></summary>

Rust handles orchestration: pipelines, async, networking, plugins, ABI to Node. Zig handles the hottest paths: codec internals, SIMD, GPU shaders. Each language wins at its layer.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full reasoning.
</details>

<details>
<summary><b>How does this compare to FFmpeg?</b></summary>

FFmpeg is a brilliant, mature, complete C codebase from 2000. `@kryxjs/core` is the **foundation** for a modern alternative built around:

- Composable npm modules instead of one monolith
- Native `async`/`await` (no callbacks, no threads to manage)
- Memory safety by default
- Real-time, AI, and GPU as first-class citizens in later packages

This single package does not replace FFmpeg — it's the toolkit Kryx packages use to build something equivalent, gradually.
</details>

<details>
<summary><b>Is this production-ready?</b></summary>

The API is stable in spirit but still pre-1.0. We may introduce breaking changes between minor versions before 1.0.

For production today: **pin the exact version** (`"0.1.2"` not `"^0.1.2"`).

After 1.0, strict semver applies.
</details>

<details>
<summary><b>How is `MediaBuffer` zero-copy?</b></summary>

Internally it's a `bytes::Bytes` (Rust's refcounted `Arc<[u8]>`). Cloning increments the refcount; slicing returns a new view into the same allocation. No `memcpy` ever happens during pipeline traversal — only at the original construction site.

```ts
const a = MediaBuffer.video(hugePayload, 0)
const b = a   // same allocation, refcount = 2
const c = b   // same allocation, refcount = 3
```
</details>

<details>
<summary><b>What about WASM / browser support?</b></summary>

Not yet, but it's on the roadmap (post-1.0). The Rust core is browser-compatible in principle; the napi-rs layer would be replaced with `wasm-bindgen`.
</details>

<details>
<summary><b>How do I report a security issue?</b></summary>

Don't open a public issue. Email **security@brashkie.dev** and follow the [Security Policy](SECURITY.md).
</details>

---

## License

[Apache-2.0](LICENSE).

Copyright © 2026 [Brashkie](https://github.com/Brashkie).

---

<div align="center">

Made with 🦀 + ⚡ for the modern multimedia web.

[Website](https://kryx.dev) · [Issues](https://github.com/Brashkie/kryx-core/issues) · [Discussions](https://github.com/Brashkie/kryx-core/discussions)

</div>