/**
 * @kryxjs/core
 *
 * Core engine for the Kryx multimedia ecosystem.
 *
 * This is the public entry point. It loads the native Rust addon (via
 * napi-rs) using a STATIC `import` (which becomes a clean `require()` in
 * CJS output and a literal `import` in ESM output), then re-exports the
 * native classes alongside the pure-TypeScript layer.
 *
 * @packageDocumentation
 */

// ─── Load the native addon ──────────────────────────────────────────────────
//
// Static import — handled correctly by tsup in both CJS and ESM output
// (the `../index.js` path stays external, no `__require` shim).
//
// eslint-disable-next-line @typescript-eslint/no-var-requires
import * as native from '../index.js'

// ─── Native addon type contracts ────────────────────────────────────────────
//
// Declared inline so we're not coupled to napi-rs's auto-generated
// `../index.d.ts` (which can change between versions / be missing during
// initial development).

/** A timestamp wrapping a Rust `Timestamp` value (PTS in some timebase). */
export interface JsTimestamp {
  readonly pts: number
  readonly isValid: boolean
}

/** Static surface of the `JsTimestamp` native class. */
export interface JsTimestampConstructor {
  new (pts: number): JsTimestamp
  none(): JsTimestamp
  zero(): JsTimestamp
}

/** A zero-copy media frame buffer wrapping a Rust `MediaBuffer`. */
export interface JsMediaBuffer {
  readonly pts: number
  readonly len: number
  readonly isEmpty: boolean
  readonly isKeyframe: boolean
  readonly isEos: boolean
  readonly mediaType: string
  readonly codecId: string
  data(): Buffer
}

/** Static surface of the `JsMediaBuffer` native class. */
export interface JsMediaBufferConstructor {
  video(data: Buffer | Uint8Array, pts: number): JsMediaBuffer
  audio(data: Buffer | Uint8Array, pts: number): JsMediaBuffer
  eosVideo(): JsMediaBuffer
}

// ─── Public re-exports (native) ─────────────────────────────────────────────

/**
 * Zero-copy media frame buffer.
 *
 * @example
 * ```ts
 * const buf = MediaBuffer.video(data, 90_000)
 * console.log(buf.pts, buf.mediaType)
 * ```
 */
export const MediaBuffer: JsMediaBufferConstructor =
  (native as unknown as { JsMediaBuffer: JsMediaBufferConstructor }).JsMediaBuffer

/**
 * Native timestamp class (rarely used directly; prefer the `Timestamp`
 * helpers from this module).
 */
export const NativeTimestamp: JsTimestampConstructor =
  (native as unknown as { JsTimestamp: JsTimestampConstructor }).JsTimestamp

/** Returns the version reported by the linked native addon. */
export function nativeAddonVersion(): string {
  return (native as unknown as { version: () => string }).version()
}

// Type alias — let consumers use `MediaBuffer` as a type too.
export type MediaBuffer = JsMediaBuffer

// ─── TypeScript layer ───────────────────────────────────────────────────────

export * from './types'
export * from './pipeline'
export * from './error'

// ─── Package version ────────────────────────────────────────────────────────

/** npm package version of `@kryxjs/core`. */
export const VERSION = '0.2.3'
