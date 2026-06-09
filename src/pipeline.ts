/**
 * @kryxjs/core — async composable pipelines
 *
 * Pure TypeScript pipeline implementation. The native Rust pipeline
 * (in `crates/kc-node`) is exposed separately; this module is the
 * portable JS-side counterpart and is what most user code interacts with.
 */

import { MediaError, PipelineError, MediaErrorKind } from './error'
import type { MediaType } from './types'

// ─── Frame abstraction ───────────────────────────────────────────────────────

/**
 * Minimal contract for a media frame passing through a pipeline.
 *
 * Both the native `JsMediaBuffer` and any user-defined buffer-like
 * object satisfy this interface, so stages can be generic across them.
 */
export interface MediaFrameLike {
  readonly pts?: number
  readonly mediaType?: MediaType
  readonly isEos?: boolean
}

// ─── Stage context ───────────────────────────────────────────────────────────

export interface StageContext {
  /** Name of the pipeline that owns the running stage. */
  readonly pipelineName: string
  /** Monotonic frame counter — increments per `process()` call. */
  readonly frameCount: number
  /** `performance.now()` when `pipeline.start()` was last called. */
  readonly startedAt: number
  /** Optional abort signal — stages should respect it for cooperative cancellation. */
  readonly signal?: AbortSignal
}

// ─── Stage trait ─────────────────────────────────────────────────────────────

/**
 * A single processing stage in a pipeline.
 *
 * - Return `[buf]` to pass the frame through.
 * - Return `[]` to drop the frame.
 * - Return `[a, b, c]` to fan out into multiple frames.
 *
 * Stages should be **stateless** when possible. State that must persist
 * across frames belongs in stage instance fields (the stage object itself
 * lives for the entire pipeline lifetime).
 */
export interface Stage<TFrame extends MediaFrameLike = MediaFrameLike> {
  /** Unique stage name — used in logs and error messages. */
  readonly name: string

  /** Process a single frame. */
  process(frame: TFrame, ctx: StageContext): Promise<readonly TFrame[]> | readonly TFrame[]

  /** Optional — called once before the first `process()` call. */
  onStart?(ctx: Pick<StageContext, 'pipelineName' | 'signal'>): Promise<void> | void

  /** Optional — called once during `pipeline.stop()` (reverse order). */
  onStop?(): Promise<void> | void

  /** Optional — declare which media types this stage accepts. */
  readonly accepts?: readonly MediaType[]
}

// ─── PipelineOptions ─────────────────────────────────────────────────────────

export interface PipelineOptions {
  /** Pipeline name (used in error messages and tracing). */
  readonly name?: string
  /** Optional abort signal for cooperative cancellation. */
  readonly signal?: AbortSignal
  /** Optional hook called on every stage error. Default: rethrow. */
  readonly onError?: (err: MediaError, stageName: string) => void
}

// ─── Pipeline ────────────────────────────────────────────────────────────────

/**
 * An ordered chain of stages that transforms media frames.
 *
 * Build with `Pipeline.builder()`; once built, the stage list is frozen.
 *
 * @example
 * ```ts
 * const pipeline = Pipeline.builder()
 *   .name('my-pipeline')
 *   .stage(myDecoder)
 *   .stage(myFilter)
 *   .build()
 *
 * await pipeline.start()
 * const out = await pipeline.process(buf)
 * await pipeline.stop()
 * ```
 */
export class Pipeline<TFrame extends MediaFrameLike = MediaFrameLike> {
  public readonly name: string
  private readonly stages: readonly Stage<TFrame>[]
  private readonly signal?: AbortSignal
  private readonly onError?: PipelineOptions['onError']

  private frameCount = 0
  private startedAt = 0
  private state: 'idle' | 'running' | 'stopped' = 'idle'

  /** @internal — use `Pipeline.builder()` */
  constructor(stages: readonly Stage<TFrame>[], options: PipelineOptions = {}) {
    if (stages.length === 0) {
      throw new PipelineError('cannot construct a pipeline with zero stages')
    }
    this.name = options.name ?? 'pipeline'
    this.stages = Object.freeze([...stages])
    this.signal = options.signal
    this.onError = options.onError
  }

  /** Start a builder for a new pipeline. */
  static builder<F extends MediaFrameLike = MediaFrameLike>(): PipelineBuilder<F> {
    return new PipelineBuilder<F>()
  }

  // ── Introspection ──────────────────────────────────────────────────────

  get stageCount(): number {
    return this.stages.length
  }

  get stageNames(): readonly string[] {
    return this.stages.map((s) => s.name)
  }

  get isRunning(): boolean {
    return this.state === 'running'
  }

  // ── Lifecycle ──────────────────────────────────────────────────────────

  /** Initialize all stages. Safe to call multiple times (idempotent). */
  async start(): Promise<void> {
    if (this.state === 'running') return
    if (this.state === 'stopped') {
      throw MediaError.closed('pipeline was already stopped; create a new one to reuse')
    }
    this.startedAt = nowMs()
    this.frameCount = 0
    for (const stage of this.stages) {
      try {
        await stage.onStart?.({ pipelineName: this.name, signal: this.signal })
      } catch (err) {
        throw this.wrapStageError(err, stage.name, 'onStart')
      }
    }
    this.state = 'running'
  }

  /** Shut down all stages in reverse order. Safe to call multiple times. */
  async stop(): Promise<void> {
    if (this.state !== 'running') {
      this.state = 'stopped'
      return
    }
    const errors: MediaError[] = []
    for (const stage of [...this.stages].reverse()) {
      try {
        await stage.onStop?.()
      } catch (err) {
        errors.push(this.wrapStageError(err, stage.name, 'onStop'))
      }
    }
    this.state = 'stopped'
    if (errors.length > 0) {
      throw MediaError.pipeline(`pipeline shutdown failed in ${errors.length} stage(s)`, {
        details: { errors: errors.map((e) => e.toJSON()) },
        cause: errors[0],
      })
    }
  }

  // ── Processing ─────────────────────────────────────────────────────────

  /** Process a single frame through every stage. */
  async process(frame: TFrame): Promise<readonly TFrame[]> {
    this.assertCanProcess()
    this.checkAborted()

    this.frameCount += 1
    const ctx: StageContext = {
      pipelineName: this.name,
      frameCount: this.frameCount,
      startedAt: this.startedAt,
      signal: this.signal,
    }

    let current: readonly TFrame[] = [frame]
    for (const stage of this.stages) {
      this.checkAborted()
      const next: TFrame[] = []
      for (const item of current) {
        try {
          const result = await stage.process(item, ctx)
          if (!Array.isArray(result) && !isReadonlyArray(result)) {
            throw MediaError.pipeline(
              `stage "${stage.name}" returned non-array result`,
              { context: stage.name },
            )
          }
          next.push(...(result as readonly TFrame[]))
        } catch (err) {
          const wrapped = this.wrapStageError(err, stage.name, 'process')
          if (this.onError) {
            this.onError(wrapped, stage.name)
            // If onError doesn't rethrow, we propagate anyway — onError is a hook,
            // not a recovery mechanism.
          }
          throw wrapped
        }
      }
      current = next
      if (current.length === 0) break
    }
    return current
  }

  /** Process multiple frames sequentially, flattening their outputs. */
  async processBatch(frames: readonly TFrame[]): Promise<readonly TFrame[]> {
    const out: TFrame[] = []
    for (const frame of frames) {
      out.push(...(await this.process(frame)))
    }
    return out
  }

  /** Drain an async iterable through the pipeline. */
  async *processStream(source: AsyncIterable<TFrame>): AsyncIterable<TFrame> {
    for await (const frame of source) {
      const out = await this.process(frame)
      for (const item of out) yield item
    }
  }

  // ── Internals ──────────────────────────────────────────────────────────

  private assertCanProcess(): void {
    if (this.state === 'stopped') {
      throw MediaError.closed(`pipeline "${this.name}" is stopped`)
    }
    if (this.state === 'idle') {
      // Auto-start for convenience — explicit `start()` is still recommended.
      this.startedAt = nowMs()
      this.state = 'running'
    }
  }

  private checkAborted(): void {
    if (this.signal?.aborted) {
      throw MediaError.from(
        this.signal.reason ?? new Error('aborted'),
        MediaErrorKind.Timeout,
      )
    }
  }

  private wrapStageError(err: unknown, stageName: string, phase: string): MediaError {
    if (MediaError.is(err)) return err
    return MediaError.pipeline(
      `stage "${stageName}" failed during ${phase}: ${describe(err)}`,
      { context: stageName, cause: err },
    )
  }
}

// ─── Builder ─────────────────────────────────────────────────────────────────

export class PipelineBuilder<TFrame extends MediaFrameLike = MediaFrameLike> {
  private _name = 'pipeline'
  private _stages: Stage<TFrame>[] = []
  private _signal?: AbortSignal
  private _onError?: PipelineOptions['onError']

  name(name: string): this {
    if (!name || typeof name !== 'string') {
      throw MediaError.pipeline('pipeline name must be a non-empty string')
    }
    this._name = name
    return this
  }

  stage(stage: Stage<TFrame>): this {
    if (!stage || typeof stage.process !== 'function' || typeof stage.name !== 'string') {
      throw MediaError.pipeline('invalid stage: must have a name and a process() function')
    }
    this._stages.push(stage)
    return this
  }

  stages(stages: readonly Stage<TFrame>[]): this {
    for (const s of stages) this.stage(s)
    return this
  }

  signal(signal: AbortSignal): this {
    this._signal = signal
    return this
  }

  onError(handler: NonNullable<PipelineOptions['onError']>): this {
    this._onError = handler
    return this
  }

  build(): Pipeline<TFrame> {
    return new Pipeline<TFrame>(this._stages, {
      name: this._name,
      signal: this._signal,
      onError: this._onError,
    })
  }
}

// ─── Built-in stages ─────────────────────────────────────────────────────────

/** A stage that emits each input frame unchanged. */
export const PassthroughStage: Stage = Object.freeze({
  name: 'passthrough',
  process<F extends MediaFrameLike>(frame: F): readonly F[] {
    return [frame]
  },
})

/** A stage that drops every input frame. */
export const DropStage: Stage = Object.freeze({
  name: 'drop',
  process(): readonly never[] {
    return []
  },
})

/**
 * A stage that counts how many frames it has seen.
 *
 * Unlike `Passthrough`/`Drop`, this stage is **stateful** so it must be
 * instantiated rather than imported as a singleton.
 */
export class CounterStage<TFrame extends MediaFrameLike = MediaFrameLike> implements Stage<TFrame> {
  readonly name = 'counter'
  private _count = 0

  process(frame: TFrame): readonly TFrame[] {
    this._count += 1
    return [frame]
  }

  get count(): number {
    return this._count
  }

  reset(): void {
    this._count = 0
  }
}

/**
 * A stage that taps the stream (side-effect only, passes frame through).
 * Useful for logging, metrics, or debugging.
 */
export function tapStage<TFrame extends MediaFrameLike>(
  name: string,
  fn: (frame: TFrame, ctx: StageContext) => void | Promise<void>,
): Stage<TFrame> {
  return {
    name,
    async process(frame, ctx) {
      await fn(frame, ctx)
      return [frame]
    },
  }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

function nowMs(): number {
  // `performance.now()` is available in Node 16+ and all modern browsers.
  // Both supported runtimes have it, so this is unconditional.
  return performance.now()
}

function isReadonlyArray(value: unknown): value is readonly unknown[] {
  return Array.isArray(value)
}

function describe(err: unknown): string {
  if (err instanceof Error) return err.message
  if (typeof err === 'string') return err
  try {
    return JSON.stringify(err)
  } catch {
    return String(err)
  }
}
