import { defineConfig } from 'tsup'

/**
 * tsup config for @kryxjs/core
 *
 * Pattern proven by @brashkie/signalis-core:
 *   - No `shims: true` (it generates a broken __require helper)
 *   - The native addon is loaded via `import * as native from '../index.js'`
 *     which becomes a literal `require('../index.js')` in CJS output and a
 *     literal `import` in ESM — both supported natively by Node.
 *   - `external` keeps tsup from bundling the native loader.
 */
export default defineConfig({
  entry: ['src/index.ts'],
  format: ['cjs', 'esm'],
  dts: true,
  splitting: false,
  sourcemap: true,
  clean: true,
  minify: false,
  target: 'node18',
  outDir: 'dist',
  // The native addon stays outside the bundle.
  external: ['../index.js', '../index.cjs', './native'],
  // Explicit extensions:
  //   .cjs → always CommonJS
  //   .mjs → always ESM
  outExtension({ format }) {
    return {
      js: format === 'cjs' ? '.cjs' : '.mjs',
    }
  },
})
