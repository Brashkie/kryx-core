/**
 * Tests for the pure TypeScript layer of @kryxjs/core.
 * These tests do NOT require the native addon to be built.
 */

import { describe, it, expect, vi } from 'vitest'
import {
  // pipeline
  Pipeline,
  PassthroughStage,
  DropStage,
  CounterStage,
  tapStage,
  // types
  MediaType,
  CodecId,
  PixelFormat,
  SampleFormat,
  Timebase,
  Timestamp,
  rescaleTs,
  ptsToMs,
  // errors
  MediaError,
  MediaErrorKind,
  PipelineError,
  BufferError,
  IoError,
  FfiError,
  SyncError,
  // version
  VERSION,
} from '../src'
import type { Stage } from '../src'

// ─── MediaError ──────────────────────────────────────────────────────────────

describe('MediaError', () => {
  it('subclasses are instanceof MediaError', () => {
    const err = new PipelineError('test')
    expect(err instanceof MediaError).toBe(true)
    expect(err instanceof PipelineError).toBe(true)
    expect(err.kind).toBe('pipeline')
  })

  it('all subclass constructors work', () => {
    expect(new BufferError('a').kind).toBe('buffer')
    expect(new PipelineError('a').kind).toBe('pipeline')
    expect(new IoError('a').kind).toBe('io')
    expect(new FfiError('a').kind).toBe('ffi')
    expect(new SyncError('a').kind).toBe('sync')
  })

  it('preserves cause chain (ES2022)', () => {
    const root = new Error('root cause')
    const err = MediaError.pipeline('wrapped', { cause: root })
    expect(err.cause).toBe(root)
  })

  it('toJSON is structured and safe to stringify', () => {
    const err = MediaError.timeout(500, { context: 'decode' })
    const json = err.toJSON()
    expect(json).toMatchObject({
      kind: 'timeout',
      message: 'operation timed out after 500ms',
      context: 'decode',
      details: { ms: 500 },
    })
    expect(() => JSON.stringify(err)).not.toThrow()
  })

  it('toJSON without context/details', () => {
    const err = new PipelineError('x')
    const json = err.toJSON()
    expect(json.context).toBeUndefined()
    expect(json.details).toBeUndefined()
  })

  it('isFatal vs isRecoverable predicates', () => {
    expect(MediaError.internal('x').isFatal).toBe(true)
    expect(MediaError.closed().isFatal).toBe(true)
    expect(MediaError.timeout(100).isRecoverable).toBe(true)
    expect(new SyncError('drift').isRecoverable).toBe(true)
    expect(new BufferError('full').isRecoverable).toBe(false)
    expect(new BufferError('x').isFatal).toBe(false)
  })

  it('MediaError.is and isKind type guards', () => {
    const err: unknown = new PipelineError('x')
    expect(MediaError.is(err)).toBe(true)
    expect(MediaError.isKind(err, 'pipeline')).toBe(true)
    expect(MediaError.isKind(err, 'buffer')).toBe(false)
    expect(MediaError.is(new Error('plain'))).toBe(false)
    expect(MediaError.is('string')).toBe(false)
    expect(MediaError.is(null)).toBe(false)
    expect(MediaError.isKind('not-error', 'pipeline')).toBe(false)
  })

  it('MediaError.from normalizes anything', () => {
    expect(MediaError.from(new Error('a')).message).toBe('a')
    expect(MediaError.from('string err').message).toBe('string err')
    expect(MediaError.from(42).message).toBe('42')
    expect(MediaError.from(null).message).toBe('null')
    expect(MediaError.from(undefined).message).toBe('undefined')
    expect(MediaError.from({ foo: 'bar' }).message).toBe('[object Object]')
    const existing = new PipelineError('keep')
    expect(MediaError.from(existing)).toBe(existing)
  })

  it('MediaError.from with custom fallback kind', () => {
    const err = MediaError.from(new Error('x'), MediaErrorKind.Buffer)
    expect(err.kind).toBe('buffer')
  })

  it('details are frozen', () => {
    const err = MediaError.timeout(100)
    expect(Object.isFrozen(err.details)).toBe(true)
  })

  it('details undefined when not provided', () => {
    const err = new PipelineError('x')
    expect(err.details).toBeUndefined()
  })

  // ── All factories ────────────────────────────────────────────────────────

  it('all factory methods produce correct kinds', () => {
    expect(MediaError.buffer('a').kind).toBe('buffer')
    expect(MediaError.pipeline('a').kind).toBe('pipeline')
    expect(MediaError.io('a').kind).toBe('io')
    expect(MediaError.ffi('a').kind).toBe('ffi')
    expect(MediaError.sync('a').kind).toBe('sync')
    expect(MediaError.unsupported('a').kind).toBe('unsupported')
    expect(MediaError.invalidTimestamp(123).kind).toBe('invalid_timestamp')
    expect(MediaError.closed().kind).toBe('closed')
    expect(MediaError.closed('custom').message).toBe('custom')
    expect(MediaError.timeout(50).kind).toBe('timeout')
    expect(MediaError.internal('a').kind).toBe('internal')
  })

  it('invalidTimestamp includes pts in details', () => {
    const err = MediaError.invalidTimestamp(-999)
    expect(err.details?.pts).toBe(-999)
    expect(err.message).toContain('-999')
  })

  it('invalidTimestamp merges existing details', () => {
    const err = MediaError.invalidTimestamp(42, { details: { source: 'demuxer' } })
    expect(err.details?.pts).toBe(42)
    expect(err.details?.source).toBe('demuxer')
  })

  it('timeout merges existing details', () => {
    const err = MediaError.timeout(100, { details: { source: 'http' } })
    expect(err.details?.ms).toBe(100)
    expect(err.details?.source).toBe('http')
  })

  it('error names match class names', () => {
    expect(new BufferError('x').name).toBe('BufferError')
    expect(new PipelineError('x').name).toBe('PipelineError')
    expect(new IoError('x').name).toBe('IoError')
    expect(new FfiError('x').name).toBe('FfiError')
    expect(new SyncError('x').name).toBe('SyncError')
  })

  it('stack trace is captured', () => {
    const err = new PipelineError('x')
    expect(err.stack).toBeDefined()
    expect(typeof err.stack).toBe('string')
  })
})

// ─── Timebase ────────────────────────────────────────────────────────────────

describe('Timebase', () => {
  it('all predefined constants exist', () => {
    expect(Timebase.VIDEO_90K.den).toBe(90_000)
    expect(Timebase.AUDIO_48K.den).toBe(48_000)
    expect(Timebase.AUDIO_44K.den).toBe(44_100)
    expect(Timebase.MILLISECOND.den).toBe(1_000)
    expect(Timebase.MICROSECOND.den).toBe(1_000_000)
  })

  it('Timebase.of validates inputs', () => {
    expect(() => Timebase.of(1, 1000)).not.toThrow()
    expect(() => Timebase.of(0, 1000)).toThrow(MediaError)
    expect(() => Timebase.of(1, 0)).toThrow(MediaError)
    expect(() => Timebase.of(-1, 1000)).toThrow(MediaError)
    expect(() => Timebase.of(1, -1000)).toThrow(MediaError)
    expect(() => Timebase.of(1.5, 1000)).toThrow(MediaError)
    expect(() => Timebase.of(1, 1000.5)).toThrow(MediaError)
  })

  it('instances are frozen and immutable', () => {
    const tb = Timebase.of(1, 1000)
    expect(Object.isFrozen(tb)).toBe(true)
  })

  it('rescale: 90kHz → milliseconds', () => {
    expect(Timebase.VIDEO_90K.rescale(90_000, Timebase.MILLISECOND)).toBe(1000)
  })

  it('rescale handles NONE timestamps', () => {
    expect(Timebase.VIDEO_90K.rescale(Timestamp.NONE, Timebase.MILLISECOND)).toBe(Timestamp.NONE)
  })

  it('rescale handles very large values (BigInt path)', () => {
    // 1 hour at 90kHz = 324_000_000 — safe to convert back to number
    const oneHour = 90_000 * 60 * 60
    expect(Timebase.VIDEO_90K.rescale(oneHour, Timebase.MILLISECOND)).toBe(3_600_000)
  })

  it('rescale throws on overflow', () => {
    // Force overflow: pts * num * targetDen must exceed MAX_SAFE_INTEGER
    // after dividing by (den * targetNum).
    // Here: MAX_SAFE_INTEGER * 1 * 1_000_000_000 / (1 * 1) → ~9e24, way past safe range.
    const huge = Number.MAX_SAFE_INTEGER
    const from = Timebase.of(1, 1)
    const to = Timebase.of(1, 1_000_000_000)
    expect(() => from.rescale(huge, to)).toThrow(MediaError)
  })

  it('toMilliseconds and toSeconds', () => {
    expect(Timebase.VIDEO_90K.toMilliseconds(90_000)).toBe(1000)
    expect(Timebase.VIDEO_90K.toSeconds(90_000)).toBe(1)
  })

  it('equals compares structurally', () => {
    expect(Timebase.of(1, 1000).equals(Timebase.MILLISECOND)).toBe(true)
    expect(Timebase.of(1, 1000).equals(Timebase.VIDEO_90K)).toBe(false)
  })

  it('toString is human-readable', () => {
    expect(Timebase.VIDEO_90K.toString()).toBe('1/90000')
  })
})

// ─── Timestamp helpers ───────────────────────────────────────────────────────

describe('Timestamp helpers', () => {
  it('isValid rejects NONE and non-finite', () => {
    expect(Timestamp.isValid(0)).toBe(true)
    expect(Timestamp.isValid(1_000_000)).toBe(true)
    expect(Timestamp.isValid(Timestamp.NONE)).toBe(false)
    expect(Timestamp.isValid(NaN)).toBe(false)
    expect(Timestamp.isValid(Infinity)).toBe(false)
    expect(Timestamp.isValid(-Infinity)).toBe(false)
  })

  it('toMilliseconds returns 0 for NONE', () => {
    expect(Timestamp.toMilliseconds(Timestamp.NONE, Timebase.VIDEO_90K)).toBe(0)
  })

  it('toMilliseconds for valid PTS', () => {
    expect(Timestamp.toMilliseconds(90_000, Timebase.VIDEO_90K)).toBe(1000)
  })

  it('toSeconds returns 0 for NONE', () => {
    expect(Timestamp.toSeconds(Timestamp.NONE, Timebase.VIDEO_90K)).toBe(0)
  })

  it('toSeconds for valid PTS', () => {
    expect(Timestamp.toSeconds(90_000, Timebase.VIDEO_90K)).toBe(1)
  })

  it('Timestamp.ZERO is valid', () => {
    expect(Timestamp.isValid(Timestamp.ZERO)).toBe(true)
  })
})

// ─── Deprecated free functions ───────────────────────────────────────────────

describe('Deprecated helpers', () => {
  it('rescaleTs still works', () => {
    expect(rescaleTs(90_000, Timebase.VIDEO_90K, Timebase.MILLISECOND)).toBe(1000)
  })

  it('ptsToMs still works', () => {
    expect(ptsToMs(90_000, Timebase.VIDEO_90K)).toBe(1000)
  })
})

// ─── Pipeline ────────────────────────────────────────────────────────────────

describe('Pipeline', () => {
  it('cannot build with zero stages', () => {
    expect(() => Pipeline.builder().build()).toThrow(PipelineError)
  })

  it('builder rejects invalid stage', () => {
    const bad = { name: 'bad' } as unknown as Stage
    expect(() => Pipeline.builder().stage(bad)).toThrow(PipelineError)
  })

  it('builder rejects stage without name', () => {
    const bad = { process: () => [] } as unknown as Stage
    expect(() => Pipeline.builder().stage(bad)).toThrow(PipelineError)
  })

  it('builder rejects null/undefined stage', () => {
    expect(() => Pipeline.builder().stage(null as unknown as Stage)).toThrow(PipelineError)
    expect(() => Pipeline.builder().stage(undefined as unknown as Stage)).toThrow(PipelineError)
  })

  it('builder rejects empty name', () => {
    expect(() => Pipeline.builder().name('')).toThrow(PipelineError)
  })

  it('builder rejects non-string name', () => {
    expect(() => Pipeline.builder().name(42 as unknown as string)).toThrow(PipelineError)
  })

  it('passthrough returns the frame', async () => {
    const pipeline = Pipeline.builder().stage(PassthroughStage).build()
    const frame = { pts: 1 }
    const out = await pipeline.process(frame)
    expect(out).toEqual([frame])
  })

  it('drop produces empty output', async () => {
    const pipeline = Pipeline.builder().stage(DropStage).build()
    const out = await pipeline.process({ pts: 1 })
    expect(out).toHaveLength(0)
  })

  it('multi-stage runs in order', async () => {
    const log: string[] = []
    const a = tapStage('a', () => { log.push('a') })
    const b = tapStage('b', () => { log.push('b') })

    const pipeline = Pipeline.builder().stage(a).stage(b).build()
    await pipeline.process({ pts: 1 })
    expect(log).toEqual(['a', 'b'])
  })

  it('isRunning reflects state', async () => {
    const pipeline = Pipeline.builder().stage(PassthroughStage).build()
    expect(pipeline.isRunning).toBe(false)
    await pipeline.start()
    expect(pipeline.isRunning).toBe(true)
    await pipeline.stop()
    expect(pipeline.isRunning).toBe(false)
  })

  it('frameCount is monotonic', async () => {
    let lastCount = 0
    const tap = tapStage('tap', (_f, ctx) => { lastCount = ctx.frameCount })

    const pipeline = Pipeline.builder().stage(tap).build()
    await pipeline.process({})
    await pipeline.process({})
    await pipeline.process({})
    expect(lastCount).toBe(3)
  })

  it('CounterStage counts frames', async () => {
    const counter = new CounterStage()
    const pipeline = Pipeline.builder().stage(counter).build()

    for (let i = 0; i < 5; i++) await pipeline.process({ pts: i })
    expect(counter.count).toBe(5)

    counter.reset()
    expect(counter.count).toBe(0)
  })

  it('processBatch handles multiple frames', async () => {
    const pipeline = Pipeline.builder().stage(PassthroughStage).build()
    const out = await pipeline.processBatch([{}, {}, {}])
    expect(out).toHaveLength(3)
  })

  it('processStream drains async iterable', async () => {
    const pipeline = Pipeline.builder().stage(PassthroughStage).build()

    async function* source() {
      yield { pts: 1 }
      yield { pts: 2 }
      yield { pts: 3 }
    }

    const out = []
    for await (const f of pipeline.processStream(source())) out.push(f)
    expect(out).toHaveLength(3)
  })

  it('fan-out stage produces multiple outputs', async () => {
    const fanout: Stage = {
      name: 'fanout',
      process(frame) { return [frame, frame, frame] },
    }
    const pipeline = Pipeline.builder().stage(fanout).build()
    const out = await pipeline.process({})
    expect(out).toHaveLength(3)
  })

  it('stage returning non-array throws', async () => {
    const bad: Stage = {
      name: 'bad',
      process() { return 'not-an-array' as unknown as readonly never[] },
    }
    const pipeline = Pipeline.builder().stage(bad).build()
    await expect(pipeline.process({})).rejects.toMatchObject({
      kind: 'pipeline',
      context: 'bad',
    })
  })

  it('wraps stage errors as MediaError', async () => {
    const bad: Stage = {
      name: 'bad',
      process() { throw new TypeError('boom') },
    }
    const pipeline = Pipeline.builder().stage(bad).build()
    await expect(pipeline.process({})).rejects.toMatchObject({
      kind: 'pipeline',
      context: 'bad',
    })
  })

  it('wraps stage errors that throw non-Error values', async () => {
    const badString: Stage = {
      name: 'bad-string',
      process(): never { throw 'string-throw' },
    }
    const p1 = Pipeline.builder().stage(badString).build()
    await expect(p1.process({})).rejects.toMatchObject({ kind: 'pipeline' })

    const badObj: Stage = {
      name: 'bad-obj',
      process(): never { throw { foo: 'bar' } },
    }
    const p2 = Pipeline.builder().stage(badObj).build()
    await expect(p2.process({})).rejects.toMatchObject({ kind: 'pipeline' })
  })

  it('wraps stage error with circular object (JSON.stringify fails)', async () => {
    const circular: Record<string, unknown> = {}
    circular.self = circular
    const bad: Stage = {
      name: 'circular',
      process(): never { throw circular },
    }
    const pipeline = Pipeline.builder().stage(bad).build()
    await expect(pipeline.process({})).rejects.toMatchObject({ kind: 'pipeline' })
  })

  it('preserves MediaError thrown by stage', async () => {
    const bad: Stage = {
      name: 'bad',
      process() { throw MediaError.unsupported('h266') },
    }
    const pipeline = Pipeline.builder().stage(bad).build()
    await expect(pipeline.process({})).rejects.toMatchObject({
      kind: 'unsupported',
    })
  })

  it('lifecycle: onStart/onStop called in correct order', async () => {
    const calls: string[] = []
    const make = (n: string): Stage => ({
      name: n,
      process(f) { return [f] },
      onStart() { calls.push(`start:${n}`) },
      onStop() { calls.push(`stop:${n}`) },
    })

    const pipeline = Pipeline.builder().stage(make('a')).stage(make('b')).build()
    await pipeline.start()
    await pipeline.process({})
    await pipeline.stop()

    expect(calls).toEqual(['start:a', 'start:b', 'stop:b', 'stop:a'])
  })

  it('start is idempotent', async () => {
    let calls = 0
    const stage: Stage = {
      name: 's',
      process(f) { return [f] },
      onStart() { calls++ },
    }
    const pipeline = Pipeline.builder().stage(stage).build()
    await pipeline.start()
    await pipeline.start()
    expect(calls).toBe(1)
  })

  it('onStart errors are wrapped', async () => {
    const stage: Stage = {
      name: 'bad-start',
      process(f) { return [f] },
      onStart() { throw new Error('cannot start') },
    }
    const pipeline = Pipeline.builder().stage(stage).build()
    await expect(pipeline.start()).rejects.toMatchObject({
      kind: 'pipeline',
      context: 'bad-start',
    })
  })

  it('onStart with MediaError is preserved', async () => {
    const stage: Stage = {
      name: 'bad-start',
      process(f) { return [f] },
      onStart() { throw MediaError.unsupported('codec') },
    }
    const pipeline = Pipeline.builder().stage(stage).build()
    await expect(pipeline.start()).rejects.toMatchObject({ kind: 'unsupported' })
  })

  it('onStop errors collected and thrown', async () => {
    const stage: Stage = {
      name: 'bad-stop',
      process(f) { return [f] },
      onStop() { throw new Error('cannot stop') },
    }
    const pipeline = Pipeline.builder().stage(stage).build()
    await pipeline.start()
    await expect(pipeline.stop()).rejects.toMatchObject({
      kind: 'pipeline',
    })
  })

  it('multiple onStop errors are aggregated', async () => {
    const make = (n: string): Stage => ({
      name: n,
      process(f) { return [f] },
      onStop() { throw new Error(`fail-${n}`) },
    })
    const pipeline = Pipeline.builder().stage(make('a')).stage(make('b')).build()
    await pipeline.start()
    try {
      await pipeline.stop()
      expect.fail('should have thrown')
    } catch (err) {
      expect(MediaError.is(err)).toBe(true)
      const details = (err as MediaError).details
      expect(Array.isArray(details?.errors)).toBe(true)
      expect((details?.errors as unknown[]).length).toBe(2)
    }
  })

  it('stop on idle is safe (no-op)', async () => {
    const pipeline = Pipeline.builder().stage(PassthroughStage).build()
    await pipeline.stop()
    expect(pipeline.isRunning).toBe(false)
  })

  it('cannot process after stop', async () => {
    const pipeline = Pipeline.builder().stage(PassthroughStage).build()
    await pipeline.start()
    await pipeline.stop()
    await expect(pipeline.process({})).rejects.toMatchObject({ kind: 'closed' })
  })

  it('cannot start after stop', async () => {
    const pipeline = Pipeline.builder().stage(PassthroughStage).build()
    await pipeline.start()
    await pipeline.stop()
    await expect(pipeline.start()).rejects.toMatchObject({ kind: 'closed' })
  })

  it('respects AbortSignal — pre-aborted', async () => {
    const ac = new AbortController()
    const pipeline = Pipeline.builder()
      .stage(PassthroughStage)
      .signal(ac.signal)
      .build()
    ac.abort('cancelled')
    await expect(pipeline.process({})).rejects.toBeInstanceOf(MediaError)
  })

  it('respects AbortSignal — aborted between stages', async () => {
    const ac = new AbortController()
    const stage1: Stage = {
      name: 's1',
      async process(f) {
        ac.abort('mid-flight')
        return [f]
      },
    }
    const pipeline = Pipeline.builder()
      .stage(stage1)
      .stage(PassthroughStage)
      .signal(ac.signal)
      .build()
    await expect(pipeline.process({})).rejects.toBeInstanceOf(MediaError)
  })

  it('onError hook is called', async () => {
    const onError = vi.fn()
    const bad: Stage = { name: 'bad', process() { throw new Error('x') } }
    const pipeline = Pipeline.builder().stage(bad).onError(onError).build()
    await expect(pipeline.process({})).rejects.toBeInstanceOf(MediaError)
    expect(onError).toHaveBeenCalledOnce()
    expect(onError.mock.calls[0][1]).toBe('bad')
  })

  it('stageNames is correct', () => {
    const pipeline = Pipeline.builder()
      .stage(PassthroughStage)
      .stage(DropStage)
      .build()
    expect(pipeline.stageNames).toEqual(['passthrough', 'drop'])
  })

  it('stageCount is correct', () => {
    const pipeline = Pipeline.builder()
      .stage(PassthroughStage)
      .stage(PassthroughStage)
      .stage(PassthroughStage)
      .build()
    expect(pipeline.stageCount).toBe(3)
  })

  it('async stage returning Promise works', async () => {
    const asyncStage: Stage = {
      name: 'async',
      async process(frame) {
        await new Promise((r) => setTimeout(r, 1))
        return [frame]
      },
    }
    const pipeline = Pipeline.builder().stage(asyncStage).build()
    const out = await pipeline.process({ pts: 1 })
    expect(out).toEqual([{ pts: 1 }])
  })

  it('default pipeline name', () => {
    const p = Pipeline.builder().stage(PassthroughStage).build()
    expect(p.name).toBe('pipeline')
  })

  it('constructor fallback name when options.name is undefined', () => {
    // Bypass the builder (which always provides a name) to exercise the
    // `options.name ?? 'pipeline'` fallback branch.
    const p = new Pipeline([PassthroughStage], {})
    expect(p.name).toBe('pipeline')
  })

  it('AbortSignal without reason throws a generic aborted error', async () => {
    const ac = new AbortController()
    const pipeline = Pipeline.builder()
      .stage(PassthroughStage)
      .signal(ac.signal)
      .build()
    ac.abort() // no reason — exercises the `?? new Error('aborted')` branch
    await expect(pipeline.process({})).rejects.toBeInstanceOf(MediaError)
  })

  it('short-circuits when all frames dropped mid-pipeline', async () => {
    let stage3Called = false
    const stage3: Stage = {
      name: 's3',
      process(f) { stage3Called = true; return [f] },
    }
    const pipeline = Pipeline.builder()
      .stage(PassthroughStage)
      .stage(DropStage)
      .stage(stage3)
      .build()
    const out = await pipeline.process({})
    expect(out).toHaveLength(0)
    expect(stage3Called).toBe(false)
  })
})

// ─── Builder fluent API ──────────────────────────────────────────────────────

describe('PipelineBuilder', () => {
  it('chains fluently', () => {
    const pipeline = Pipeline.builder()
      .name('chained')
      .stage(PassthroughStage)
      .stage(PassthroughStage)
      .build()
    expect(pipeline.name).toBe('chained')
    expect(pipeline.stageCount).toBe(2)
  })

  it('stages() adds an array', () => {
    const pipeline = Pipeline.builder()
      .stages([PassthroughStage, PassthroughStage, PassthroughStage])
      .build()
    expect(pipeline.stageCount).toBe(3)
  })
})

// ─── Type re-exports smoke ───────────────────────────────────────────────────

describe('Type re-exports', () => {
  it('MediaType constants exist', () => {
    expect(MediaType.Video).toBe('video')
    expect(MediaType.Audio).toBe('audio')
    expect(MediaType.Subtitle).toBe('subtitle')
    expect(MediaType.Data).toBe('data')
  })

  it('CodecId constants exist', () => {
    expect(CodecId.H264).toBe('h264')
    expect(CodecId.Aac).toBe('aac')
    expect(CodecId.Unknown).toBe('unknown')
  })

  it('PixelFormat constants exist', () => {
    expect(PixelFormat.Yuv420p).toBe('yuv420p')
    expect(PixelFormat.Rgba).toBe('rgba')
  })

  it('SampleFormat constants exist', () => {
    expect(SampleFormat.S16).toBe('s16')
    expect(SampleFormat.F32).toBe('f32')
  })

  it('VERSION exists', () => {
    expect(typeof VERSION).toBe('string')
    expect(VERSION.length).toBeGreaterThan(0)
  })
})

// ─── Native bridge ───────────────────────────────────────────────────────────

import { MediaBuffer, nativeAddonVersion } from '../src'

/**
 * These tests exercise the REAL native addon (the `.node` binary built by
 * `napi build`). They previously used a JS mock injected via Module._load,
 * but since we now use `import * as native from '../index.js'` (static ESM
 * import) instead of `require()` (dynamic), Module._load can't intercept it
 * and the real binary is loaded.
 *
 * That's fine — the real binary is faster and more accurate than a mock.
 * The napi-rs binding expects `Array<number>` (not Buffer/Uint8Array) for
 * data payloads — see index.d.ts for the actual signatures.
 */
describe('Native bridge', () => {
  it('nativeAddonVersion returns a non-empty string', () => {
    const v = nativeAddonVersion()
    expect(typeof v).toBe('string')
    expect(v.length).toBeGreaterThan(0)
  })

  it('MediaBuffer.video creates a buffer', () => {
    const buf = MediaBuffer.video(Buffer.from([1, 2, 3]), 90_000)
    expect(buf.pts).toBe(90_000)
    expect(buf.len).toBe(3)
    expect(buf.mediaType).toBe('video')
    expect(buf.codecId).toBe('h264')
    expect(buf.isEmpty).toBe(false)
    expect(buf.isEos).toBe(false)
  })

  it('MediaBuffer.audio creates a buffer', () => {
    const buf = MediaBuffer.audio(Buffer.from([1, 2]), 48_000)
    expect(buf.mediaType).toBe('audio')
    expect(buf.codecId).toBe('aac')
    expect(buf.len).toBe(2)
  })

  it('MediaBuffer.eosVideo creates EOS sentinel', () => {
    const eos = MediaBuffer.eosVideo()
    expect(eos.isEos).toBe(true)
    expect(eos.isEmpty).toBe(true)
    expect(eos.len).toBe(0)
  })

  it('MediaBuffer.data() returns payload', () => {
    const payload = Buffer.from([0xaa, 0xbb, 0xcc])
    const buf = MediaBuffer.video(payload, 0)
    const data = buf.data()
    expect(Buffer.from(data)).toEqual(payload)
  })

  it('MediaBuffer accepts Uint8Array', () => {
    const arr = new Uint8Array([1, 2, 3, 4])
    const buf = MediaBuffer.video(arr, 0)
    expect(buf.len).toBe(4)
  })
})