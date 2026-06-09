# Migration guide: `@brashkie/media-core` → `@kryxjs/core`

This document explains how to upgrade from the legacy `@brashkie/media-core` package to `@kryxjs/core`.

## TL;DR

```bash
npm uninstall @brashkie/media-core
npm install @kryxjs/core
```

Update your imports:

```diff
- import { MediaBuffer, Pipeline, MediaError } from '@brashkie/media-core'
+ import { MediaBuffer, Pipeline, MediaError } from '@kryxjs/core'
```

That's it. The public TypeScript API is **identical**.

## What changed

| Aspect | Old | New |
|--------|-----|-----|
| npm package | `@brashkie/media-core` | `@kryxjs/core` |
| Repository | `Brashkie/media-core` | `Brashkie/kryx-core` |
| Per-platform sub-packages | `@brashkie/media-core-linux-x64-gnu` | `@kryxjs/core-linux-x64-gnu` |
| Rust crate names | `mc-core`, `mc-node` | `kc-core`, `kc-node` |
| napi `Buffer` types | `Array<number>` (binding bug) | `Buffer \| Uint8Array` (correct) |
| TypeScript API | identical | identical |

## The Buffer fix

In `@brashkie/media-core@0.1.4`, the napi binding for `MediaBuffer.video()` and `MediaBuffer.audio()` exposed `Array<number>` instead of `Buffer`. This forced consumers to do ugly casts:

```ts
// Old (media-core@0.1.4) — ugly workaround
const buf = MediaBuffer.video([1, 2, 3] as unknown as Buffer, 0)

// data() also returned Array<number>
const data = buf.data() as unknown as number[]
```

In `@kryxjs/core@0.1.0`, the binding correctly uses `napi::bindgen_prelude::Buffer`:

```ts
// New (kryx-core@0.1.0) — natural and ergonomic
const buf = MediaBuffer.video(Buffer.from([1, 2, 3]), 0)
const buf2 = MediaBuffer.video(new Uint8Array([1, 2, 3]), 0)

// data() returns Buffer
const data: Buffer = buf.data()
```

If you were already using `Buffer.from(...)` (the natural way), no code change needed — just update the import.

## What did NOT change

- Public TypeScript API (every class, function, type, signature)
- `MediaError` hierarchy and all error kinds
- `Pipeline` builder pattern and all built-in stages
- `Timebase` / `Timestamp` semantics
- ESM + CJS dual format
- Per-platform native binaries (7 platforms)
- Node.js ≥18 requirement
- Test coverage and behavior

## Status of `@brashkie/media-core`

- `@brashkie/media-core@0.1.4` is the **last functional version**.
- It will be **deprecated** on npm with a notice pointing to `@kryxjs/core`.
- It will **not** receive new features. Only critical security fixes (if any) until the end of 2026.
- Versions 0.1.0–0.1.3 are already deprecated as broken.

## Step-by-step migration

```powershell
# 1. Find usages in your project
grep -r "@brashkie/media-core" src/

# 2. Update imports
# Use find/replace in your editor:
#   @brashkie/media-core  →  @kryxjs/core

# 3. Update package.json
npm uninstall @brashkie/media-core
npm install @kryxjs/core

# 4. Clean up any `as unknown as Buffer` casts you had (optional but recommended)

# 5. Run your tests
npm test
```

## Compatibility table

| Your code with `@brashkie/media-core@0.1.4` | Works on `@kryxjs/core@0.1.0`? |
|--------------------------------------------|------------------------------|
| `MediaBuffer.video(Buffer.from([1,2,3]), 0)` | ✅ Yes (was already natural) |
| `MediaBuffer.video([1,2,3] as unknown as Buffer, 0)` | ⚠ Works but unnecessary cast — clean up |
| `MediaBuffer.video(new Uint8Array([1,2,3]), 0)` | ✅ Yes (new — also supported) |
| `buf.data() as unknown as number[]` | ⚠ Works but unnecessary cast — `buf.data()` now returns `Buffer` |
| `Pipeline.builder().stage(...).build()` | ✅ Yes (identical) |
| `new MediaError(...)` and subclasses | ✅ Yes (identical) |

## Need help?

[Open an issue on GitHub](https://github.com/Brashkie/kryx-core/issues) or [start a discussion](https://github.com/Brashkie/kryx-core/discussions).
