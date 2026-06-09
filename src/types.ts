/**
 * @kryxjs/core — shared types
 *
 * TypeScript-side mirrors of the Rust types exposed via napi-rs.
 * Keep these stable; every other `@kryxjs/*` package depends on them.
 */

import { MediaError, MediaErrorKind } from './error'

// ─── String discriminants ─────────────────────────────────────────────────────

export const MediaType = {
  Video: 'video',
  Audio: 'audio',
  Subtitle: 'subtitle',
  Data: 'data',
} as const
export type MediaType = (typeof MediaType)[keyof typeof MediaType]

export const CodecId = {
  // Video
  H264: 'h264',
  H265: 'h265',
  Av1: 'av1',
  Vp8: 'vp8',
  Vp9: 'vp9',
  // Audio
  Aac: 'aac',
  Mp3: 'mp3',
  Opus: 'opus',
  Flac: 'flac',
  Pcm16Le: 'pcm_s16le',
  PcmF32Le: 'pcm_f32le',
  // Subtitle
  Srt: 'srt',
  Ass: 'ass',
  WebVtt: 'webvtt',
  Unknown: 'unknown',
} as const
export type CodecId = (typeof CodecId)[keyof typeof CodecId]

export const PixelFormat = {
  Yuv420p: 'yuv420p',
  Yuv422p: 'yuv422p',
  Yuv444p: 'yuv444p',
  Rgba: 'rgba',
  Bgra: 'bgra',
  Rgb24: 'rgb24',
  Nv12: 'nv12',
  Yuv420p10Le: 'yuv420p10le',
} as const
export type PixelFormat = (typeof PixelFormat)[keyof typeof PixelFormat]

export const SampleFormat = {
  S16: 's16',
  S32: 's32',
  F32: 'f32',
  F64: 'f64',
  S16P: 's16p',
  F32P: 'f32p',
} as const
export type SampleFormat = (typeof SampleFormat)[keyof typeof SampleFormat]

// ─── Metadata ────────────────────────────────────────────────────────────────

export interface VideoMeta {
  readonly width: number
  readonly height: number
  readonly pixelFormat: PixelFormat
  readonly darNum: number
  readonly darDen: number
}

export interface AudioMeta {
  readonly sampleRate: number
  readonly channels: number
  readonly sampleFormat: SampleFormat
  readonly nbSamples: number
}

export interface StreamInfo {
  readonly index: number
  readonly mediaType: MediaType
  readonly codecId: CodecId
  readonly timebase: Timebase
  readonly durationPts?: number
}

// ─── Timebase ────────────────────────────────────────────────────────────────

/**
 * A rational number representing time units per second.
 *
 * Use the static constants (`Timebase.VIDEO_90K`, `Timebase.AUDIO_48K`, ...)
 * for common cases. Use `Timebase.of(num, den)` for custom timebases.
 *
 * Instances are immutable and structurally equal — compare with `equals()`.
 */
export class Timebase {
  static readonly VIDEO_90K: Timebase = new Timebase(1, 90_000)
  static readonly AUDIO_48K: Timebase = new Timebase(1, 48_000)
  static readonly AUDIO_44K: Timebase = new Timebase(1, 44_100)
  static readonly MILLISECOND: Timebase = new Timebase(1, 1_000)
  static readonly MICROSECOND: Timebase = new Timebase(1, 1_000_000)

  readonly num: number
  readonly den: number

  private constructor(num: number, den: number) {
    this.num = num
    this.den = den
    Object.freeze(this)
  }

  /** Construct a custom timebase. Throws if `num <= 0` or `den <= 0`. */
  static of(num: number, den: number): Timebase {
    if (!Number.isInteger(num) || !Number.isInteger(den)) {
      throw MediaError.from(new RangeError('timebase num/den must be integers'), MediaErrorKind.Internal)
    }
    if (num <= 0 || den <= 0) {
      throw MediaError.from(
        new RangeError(`timebase must be positive: got ${num}/${den}`),
        MediaErrorKind.Internal,
      )
    }
    return new Timebase(num, den)
  }

  equals(other: Timebase): boolean {
    return this.num === other.num && this.den === other.den
  }

  toString(): string {
    return `${this.num}/${this.den}`
  }

  /** Convert a PTS in this timebase to seconds (float). */
  toSeconds(pts: number): number {
    return (pts * this.num) / this.den
  }

  /** Convert a PTS in this timebase to milliseconds (float). */
  toMilliseconds(pts: number): number {
    return (pts * this.num * 1000) / this.den
  }

  /** Rescale a PTS from this timebase into `target` (rounded to integer). */
  rescale(pts: number, target: Timebase): number {
    if (!Timestamp.isValid(pts)) return Timestamp.NONE
    // Use BigInt to avoid precision loss on large PTS values.
    const result =
      (BigInt(pts) * BigInt(this.num) * BigInt(target.den)) /
      (BigInt(this.den) * BigInt(target.num))
    // Clamp to safe integer range when converting back to number.
    if (result > BigInt(Number.MAX_SAFE_INTEGER) || result < BigInt(Number.MIN_SAFE_INTEGER)) {
      throw MediaError.internal(`rescale overflow: ${pts} ${this} -> ${target}`)
    }
    return Number(result)
  }
}

// ─── Timestamp ───────────────────────────────────────────────────────────────

/**
 * Static helpers for working with timestamps.
 * Timestamps themselves are stored as plain `number` (matching the napi binding).
 */
export const Timestamp = {
  /** Sentinel value representing an unknown / unset timestamp. */
  NONE: Number.MIN_SAFE_INTEGER,

  /** Zero timestamp — start of stream. */
  ZERO: 0,

  /** Returns `true` if `pts` is a valid timestamp (not `NONE`). */
  isValid(pts: number): boolean {
    return Number.isFinite(pts) && pts !== Timestamp.NONE
  },

  /** Convert a PTS + timebase to milliseconds. */
  toMilliseconds(pts: number, tb: Timebase): number {
    if (!Timestamp.isValid(pts)) return 0
    return tb.toMilliseconds(pts)
  },

  /** Convert a PTS + timebase to seconds. */
  toSeconds(pts: number, tb: Timebase): number {
    if (!Timestamp.isValid(pts)) return 0
    return tb.toSeconds(pts)
  },
} as const

// ─── Free-function aliases (convenience) ─────────────────────────────────────

/** @deprecated — use `tb.rescale(pts, target)` instead. */
export function rescaleTs(pts: number, from: Timebase, to: Timebase): number {
  return from.rescale(pts, to)
}

/** @deprecated — use `tb.toMilliseconds(pts)` instead. */
export function ptsToMs(pts: number, tb: Timebase): number {
  return tb.toMilliseconds(pts)
}
