/**
 * Global vitest setup — mocks the native addon for all tests.
 *
 * The native `.node` binary is built by `napi build` into the project root.
 * For unit tests we replace it with a pure-JS implementation so:
 *   1. Tests don't require building Rust first.
 *   2. Coverage isn't skewed by uncovered native types.
 *   3. Behavior is deterministic across platforms.
 *
 * We intercept at the `require()` level (not via `vi.mock`) because
 * `src/index.ts` loads the addon with a runtime `require('../index.js')`,
 * which Vitest's static-import mocking cannot reach.
 */

import { resolve, dirname } from 'node:path'
import Module from 'node:module'

// ─── Pure-JS mock of the native addon ────────────────────────────────────────

class JsTimestamp {
  readonly pts: number
  readonly isValid: boolean
  constructor(pts: number) {
    this.pts = pts
    this.isValid = pts !== Number.MIN_SAFE_INTEGER
  }
  static none(): JsTimestamp {
    return new JsTimestamp(Number.MIN_SAFE_INTEGER)
  }
  static zero(): JsTimestamp {
    return new JsTimestamp(0)
  }
}

class JsMediaBuffer {
  readonly pts: number
  readonly len: number
  readonly isEmpty: boolean
  readonly isKeyframe: boolean
  readonly isEos: boolean
  readonly mediaType: string
  readonly codecId: string
  private readonly _data: Buffer

  private constructor(
    data: Buffer,
    pts: number,
    mediaType: string,
    codecId: string,
    eos = false,
  ) {
    this._data = data
    this.pts = pts
    this.len = data.length
    this.isEmpty = data.length === 0
    this.isKeyframe = false
    this.isEos = eos
    this.mediaType = mediaType
    this.codecId = codecId
  }

  static video(data: Buffer | Uint8Array, pts: number): JsMediaBuffer {
    return new JsMediaBuffer(Buffer.from(data), pts, 'video', 'h264')
  }
  static audio(data: Buffer | Uint8Array, pts: number): JsMediaBuffer {
    return new JsMediaBuffer(Buffer.from(data), pts, 'audio', 'aac')
  }
  static eosVideo(): JsMediaBuffer {
    return new JsMediaBuffer(Buffer.alloc(0), 0, 'video', 'h264', true)
  }

  data(): Buffer {
    return this._data
  }
}

const mockedAddon = {
  JsTimestamp,
  JsMediaBuffer,
  version: () => '0.1.0-mock',
}

// ─── Patch require() to intercept the native addon path ──────────────────────

// Resolve the absolute path of the project root's index.js.
// `__dirname` is available because vitest evaluates this file in a CJS-compatible
// context regardless of the package's "type" field.
const PROJECT_ROOT = resolve(__dirname, '..')
const NATIVE_ADDON_PATH = resolve(PROJECT_ROOT, 'index.js')
const NATIVE_ADDON_PATH_NO_EXT = NATIVE_ADDON_PATH.replace(/\.js$/, '')

// Hook into Node's module resolver: when ANY code in this process tries to
// require '../index.js' (or its resolved absolute path), return our mock.
const ModuleAny = Module as unknown as {
  _load: (request: string, parent: unknown, isMain: boolean) => unknown
}
const originalLoad = ModuleAny._load

ModuleAny._load = function patchedLoad(
  request: string,
  parent: unknown,
  isMain: boolean,
) {
  // Try to resolve the request relative to the parent's directory.
  try {
    const parentFile =
      (parent as { filename?: string } | null | undefined)?.filename ?? ''
    const parentDir = parentFile ? dirname(parentFile) : process.cwd()
    const resolved = request.startsWith('.')
      ? resolve(parentDir, request)
      : request

    if (
      resolved === NATIVE_ADDON_PATH ||
      resolved === NATIVE_ADDON_PATH_NO_EXT ||
      resolved + '.js' === NATIVE_ADDON_PATH
    ) {
      return mockedAddon
    }
  } catch {
    // fall through to the original loader
  }

  return originalLoad.call(this, request, parent, isMain)
}