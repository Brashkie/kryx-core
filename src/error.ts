/**
 * @kryxjs/core — error types
 *
 * Unified error hierarchy mirroring the Rust `MediaError` enum.
 * Every error has a discriminant `kind` for type-safe handling,
 * an optional `context` for debugging, and a `cause` chain.
 */

// ─── Discriminants ───────────────────────────────────────────────────────────

export const MediaErrorKind = {
  Buffer: 'buffer',
  Pipeline: 'pipeline',
  Io: 'io',
  Ffi: 'ffi',
  Sync: 'sync',
  Unsupported: 'unsupported',
  InvalidTimestamp: 'invalid_timestamp',
  Closed: 'closed',
  Timeout: 'timeout',
  Internal: 'internal',
} as const

export type MediaErrorKind = (typeof MediaErrorKind)[keyof typeof MediaErrorKind]

// ─── Options ─────────────────────────────────────────────────────────────────

export interface MediaErrorOptions {
  /** Optional context (stage name, function name, etc.). */
  readonly context?: string
  /** Optional underlying cause (chain via `Error.cause`). */
  readonly cause?: unknown
  /** Optional structured details for programmatic inspection. */
  readonly details?: Readonly<Record<string, unknown>>
}

// ─── Base class ──────────────────────────────────────────────────────────────

/**
 * Base error class for all `@kryxjs/core` errors.
 *
 * Always prefer the specialized factories (`MediaError.buffer`, etc.)
 * or the subclasses (`BufferError`, `PipelineError`, ...) over this
 * constructor directly.
 *
 * @example
 * ```ts
 * try {
 *   await pipeline.process(buf)
 * } catch (err) {
 *   if (MediaError.is(err) && err.kind === 'pipeline') {
 *     // handle pipeline error
 *   }
 * }
 * ```
 */
export class MediaError extends Error {
  public readonly kind: MediaErrorKind
  public readonly context?: string
  public readonly details?: Readonly<Record<string, unknown>>

  constructor(kind: MediaErrorKind, message: string, options: MediaErrorOptions = {}) {
    // `cause` is part of Error since ES2022 — passed through `ErrorOptions`.
    super(message, options.cause !== undefined ? { cause: options.cause } : undefined)
    this.name = this.constructor.name
    this.kind = kind
    this.context = options.context
    this.details = options.details ? Object.freeze({ ...options.details }) : undefined

    // Preserve a clean stack trace where V8 supports it.
    const errorCtor = Error as unknown as {
      captureStackTrace?: (target: object, constructorOpt?: (...args: unknown[]) => unknown) => void
    }
    if (typeof errorCtor.captureStackTrace === 'function') {
      errorCtor.captureStackTrace(this, this.constructor as (...args: unknown[]) => unknown)
    }
  }

  // ── Type guards ─────────────────────────────────────────────────────────

  /** Type guard — returns `true` if `value` is a `MediaError`. */
  static is(value: unknown): value is MediaError {
    return value instanceof MediaError
  }

  /** Type guard — returns `true` if `value` is a `MediaError` with the given kind. */
  static isKind<K extends MediaErrorKind>(
    value: unknown,
    kind: K,
  ): value is MediaError & { kind: K } {
    return MediaError.is(value) && value.kind === kind
  }

  // ── Predicates ──────────────────────────────────────────────────────────

  /** Errors that callers may reasonably retry from. */
  get isRecoverable(): boolean {
    return this.kind === MediaErrorKind.Sync || this.kind === MediaErrorKind.Timeout
  }

  /** Errors that indicate a bug or unrecoverable runtime state. */
  get isFatal(): boolean {
    return this.kind === MediaErrorKind.Internal || this.kind === MediaErrorKind.Closed
  }

  // ── Serialization ───────────────────────────────────────────────────────

  /**
   * Serialize to a plain object — safe to `JSON.stringify`.
   * Excludes the stack trace by default.
   */
  toJSON(): {
    name: string
    kind: MediaErrorKind
    message: string
    context?: string
    details?: Readonly<Record<string, unknown>>
  } {
    return {
      name: this.name,
      kind: this.kind,
      message: this.message,
      ...(this.context !== undefined && { context: this.context }),
      ...(this.details !== undefined && { details: this.details }),
    }
  }

  // ── Factories ───────────────────────────────────────────────────────────

  static buffer(message: string, options?: MediaErrorOptions): BufferError {
    return new BufferError(message, options)
  }

  static pipeline(message: string, options?: MediaErrorOptions): PipelineError {
    return new PipelineError(message, options)
  }

  static io(message: string, options?: MediaErrorOptions): IoError {
    return new IoError(message, options)
  }

  static ffi(message: string, options?: MediaErrorOptions): FfiError {
    return new FfiError(message, options)
  }

  static sync(message: string, options?: MediaErrorOptions): SyncError {
    return new SyncError(message, options)
  }

  static unsupported(message: string, options?: MediaErrorOptions): MediaError {
    return new MediaError(MediaErrorKind.Unsupported, message, options)
  }

  static invalidTimestamp(pts: number, options?: MediaErrorOptions): MediaError {
    return new MediaError(MediaErrorKind.InvalidTimestamp, `invalid timestamp: ${pts}`, {
      ...options,
      details: { ...options?.details, pts },
    })
  }

  static closed(message = 'resource is closed', options?: MediaErrorOptions): MediaError {
    return new MediaError(MediaErrorKind.Closed, message, options)
  }

  static timeout(ms: number, options?: MediaErrorOptions): MediaError {
    return new MediaError(MediaErrorKind.Timeout, `operation timed out after ${ms}ms`, {
      ...options,
      details: { ...options?.details, ms },
    })
  }

  static internal(message: string, options?: MediaErrorOptions): MediaError {
    return new MediaError(MediaErrorKind.Internal, message, options)
  }

  /** Normalize an unknown thrown value into a `MediaError`. */
  static from(value: unknown, fallbackKind: MediaErrorKind = MediaErrorKind.Internal): MediaError {
    if (MediaError.is(value)) return value
    if (value instanceof Error) {
      return new MediaError(fallbackKind, value.message, { cause: value })
    }
    return new MediaError(fallbackKind, String(value))
  }
}

// ─── Specialized subclasses ──────────────────────────────────────────────────

export class BufferError extends MediaError {
  constructor(message: string, options?: MediaErrorOptions) {
    super(MediaErrorKind.Buffer, message, options)
  }
}

export class PipelineError extends MediaError {
  constructor(message: string, options?: MediaErrorOptions) {
    super(MediaErrorKind.Pipeline, message, options)
  }
}

export class IoError extends MediaError {
  constructor(message: string, options?: MediaErrorOptions) {
    super(MediaErrorKind.Io, message, options)
  }
}

export class FfiError extends MediaError {
  constructor(message: string, options?: MediaErrorOptions) {
    super(MediaErrorKind.Ffi, message, options)
  }
}

export class SyncError extends MediaError {
  constructor(message: string, options?: MediaErrorOptions) {
    super(MediaErrorKind.Sync, message, options)
  }
}
