# Architecture

This document explains the design of `@kryxjs/core` — *why* the code is structured the way it is, not just *how*. If you're contributing or building on top of it, read this first.

---

## Design goals

1. **Zero-copy by default.** Multimedia frames are large. Copying them across pipeline stages is wasteful. Every type in this crate is designed so cloning is a refcount bump, not a memcpy.

2. **Async-native.** Modern media workloads are I/O-bound: network sources, file I/O, GPU dispatches. Synchronous code blocks all of these. We build on `tokio` from the ground up.

3. **Composable, not monolithic.** FFmpeg is a giant linked binary. `@kryxjs/core` is a base; everything else (`media-codecs`, `media-stream`, ...) is an independent npm package that depends on it.

4. **Type-safe at every boundary.** Rust catches errors at compile time. napi-rs auto-generates `.d.ts` so the TypeScript layer is also fully typed. No `any`.

5. **Right language for the layer.**
   - **Rust** for orchestration, async, memory safety, ABI with Node.
   - **Zig** for ultra-low-level: codecs, SIMD, GPU shaders, container parsers.
   - **TypeScript** for the public API surface and developer ergonomics.

---

## Module layout

```
crates/
├── kc-core/                  # Pure Rust — no Node dependencies
│   └── src/
│       ├── lib.rs            # Public surface re-exports
│       ├── buffer/           # MediaBuffer, FrameFlags, builder
│       ├── pipeline/         # Pipeline, Stage trait, built-ins
│       ├── sync/             # MasterClock, StreamClock
│       ├── io/               # MediaSource, MediaSink traits
│       ├── ffi/              # Zig bridge (feature-gated)
│       ├── types/            # Timestamp, Timebase, CodecId, ...
│       ├── error/            # MediaError + sub-error types
│       └── utils/            # Helpers (align, human_size)
│
└── kc-node/                  # napi-rs bindings (cdylib)
    └── src/lib.rs            # #[napi] wrappers exposing kc-core

src/                          # TypeScript layer
├── index.ts                  # Public API
├── pipeline.ts               # JS-side Pipeline (mirrors Rust)
├── types.ts                  # Timebase class, type constants
└── error.ts                  # MediaError hierarchy
```

**Two crates, deliberately:**

- `kc-core` is a regular Rust library, usable standalone in any Rust project (no Node, no napi). Other Rust-only consumers can depend on it directly.
- `kc-node` is a thin `cdylib` that only contains napi-rs annotations. This separation means the core logic gets reviewed by Rust tooling (clippy, miri, tests) without napi noise.

---

## The pipeline model

A `Pipeline` is a **linear** chain of stages. Each stage is an async function `(frame) -> [frames]`:

```
   ┌──────────┐    ┌──────────┐    ┌──────────┐
   │  Stage   │───▶│  Stage   │───▶│  Stage   │
   │  decode  │    │  resize  │    │  encode  │
   └──────────┘    └──────────┘    └──────────┘
       │              │  ▲              │
       └──── fan-out ─┘  └── drop ──────┘
```

### Why linear, not a DAG?

Most pipelines *are* linear. DAGs are useful but rare, and they add huge complexity (cycle detection, scheduling, backpressure routing). When DAGs are needed, a higher-level package (`media-graph`?) can compose linear pipelines.

### Why per-frame, not chunked?

The unit of work in multimedia is a frame (video) or packet (audio). Working frame-by-frame:
- Matches the natural granularity of codecs.
- Makes backpressure trivial (one frame in, one operation).
- Enables fan-out (`return [a, b]`) and drop (`return []`) naturally.

For batch operations, `pipeline.processBatch([f1, f2, ...])` is provided — but internally it still loops frame-by-frame.

### Why optional `onStart`/`onStop`?

Some stages need resources (GPU context, codec init, file handles). Forcing every stage to implement lifecycle hooks would be noise; making them optional means stateless stages stay trivial.

---

## `MediaBuffer` — zero-copy semantics

```rust
pub struct MediaBuffer {
    data: Bytes,           // refcounted byte buffer
    pts: Timestamp,
    timebase: Timebase,
    media_type: MediaType,
    codec_id: CodecId,
    flags: FrameFlags,
    meta: Arc<FrameMeta>,
    // ...
}
```

The `Bytes` type from the `bytes` crate is internally `Arc<[u8]>`. Cloning increments a refcount; slicing returns a new view into the same allocation without copying. This means:

```rust
let frame = MediaBuffer::builder(MediaType::Video)
    .data(huge_vector)  // moves once into Bytes
    .build()?;

let clone1 = frame.clone();      // 0 bytes copied
let clone2 = frame.clone();      // 0 bytes copied
let half   = frame.slice(0, 1024)?;  // 0 bytes copied
```

Same applies on the TypeScript side: `JsMediaBuffer` is just a handle to the Rust `MediaBuffer`.

---

## The FFI bridge

Down the line, hot paths (codec decode/encode, SIMD pixel ops) need to live in Zig. The bridge is in `crates/kc-core/src/ffi/`:

```rust
extern "C" {
    pub fn zig_buffer_free(buf: ZigBufferOut);
    pub fn zig_version() -> *const c_char;
    pub fn zig_probe() -> ZigResult;
}
```

These symbols are gated behind the `link-zig` feature. **Without** the feature, the same symbols are implemented as Rust stubs so the crate compiles and tests pass without any Zig library present.

### Why feature-gated?

- Developers can clone, build, and test the entire ecosystem with **only Rust + Node** installed. No Zig toolchain needed.
- CI is simpler — Zig linkage is opt-in per package.
- Each downstream package (`media-codecs`, `media-gpu`) enables `link-zig` and provides its own Zig library.

---

## Error design

`MediaError` is a single discriminated enum with sub-error types. From TypeScript:

```ts
if (MediaError.isKind(err, 'pipeline')) {
  // narrowed: err.kind === 'pipeline'
  console.log(err.context, err.details, err.cause)
}
```

### Why a flat `kind` instead of just `instanceof`?

`instanceof` works for the JS-side hierarchy, but errors crossing FFI lose their JS class. The `kind` discriminant is a string that survives JSON serialization, FFI boundaries, log files, and network protocols. Use `instanceof` when you have a live JS error; use `kind` for everything else.

### Why an `Error.cause` chain?

ES2022 added `Error.cause`. We use it to preserve the original error when wrapping (e.g. stage error → pipeline error → user-facing error). The chain is JSON-serializable via `toJSON()`.

---

## Async runtime

The Rust core uses `tokio` for async, but exposes types that don't *require* tokio (the `Stage` trait uses `async_trait`, which is runtime-agnostic).

The TypeScript layer is pure async/await — no runtime dependency at all. The JS `Pipeline` is a different implementation from the Rust `Pipeline`; they don't share code, but they share the same conceptual model.

### Why a separate JS pipeline?

The Rust pipeline lives across FFI; every method call crosses napi. For pure-JS use cases (composing stages that don't touch the native side), a JS-native pipeline is dramatically faster and avoids serialization. When native stages are involved, individual `process()` calls cross FFI but the orchestration stays in JS.

---

## Timestamps and timebases

Multimedia timestamps are rational numbers (`pts × num / den`). Floats lose precision; integers wrap. We use:

- `Timestamp` as plain `i64`/`number` (cheap, copyable).
- `Timebase` as an immutable struct with `num` and `den`.
- BigInt internally during rescaling to avoid loss of precision on long PTS values (1+ hour at 90kHz = 324M, well within `i64` but tricky to multiply by other factors).

Standard timebases provided as constants:

```ts
Timebase.VIDEO_90K   // 1/90000  — MPEG video
Timebase.AUDIO_48K   // 1/48000  — modern audio
Timebase.AUDIO_44K   // 1/44100  — CD quality
Timebase.MILLISECOND // 1/1000
Timebase.MICROSECOND // 1/1_000_000
```

Custom: `Timebase.of(num, den)`.

---

## Backpressure & cancellation

- **AbortSignal**: every Pipeline accepts an optional `AbortSignal`. Between stages and on every frame entry, we check `signal.aborted` and throw a `MediaError(timeout)`.
- **Backpressure**: a stage that returns `[]` removes a frame; a stage that does `await` naturally creates backpressure (the producer awaits). For pull-based streams, use `processStream(asyncIterable)`.

---

## What's NOT in scope

`@kryxjs/core` deliberately does NOT include:

- Codec implementations → `@kryxjs/codecs` (Zig)
- Container parsing → `@kryxjs/containers` (Zig)
- GPU shaders → `@kryxjs/gpu`
- Streaming protocols → `@kryxjs/stream`
- AI models → `@kryxjs/ai`
- CLI tools → `@kryxjs/cli`
- Final FFmpeg-style facade → `kryx`

This keeps `@kryxjs/core` small, focused, and stable. Every other package depends on it; we cannot afford to put codec experiments in here.

---

## Performance philosophy

We optimize for:
1. **Memory** before CPU. Allocations dominate at this layer.
2. **Allocator pressure** before raw throughput. Reuse > realloc.
3. **Real-world workloads** before microbenchmarks.

We do NOT optimize for:
- 0.1% gains that hurt readability.
- Premature SIMD in orchestration code (save it for codecs).
- Lock-free everything (only where contention is measured).

---

## See also

- [ROADMAP.md](ROADMAP.md) — what's coming next
- [CONTRIBUTING.md](../CONTRIBUTING.md) — how to contribute
- [API docs](https://docs.kryx.dev/core) — full API reference
