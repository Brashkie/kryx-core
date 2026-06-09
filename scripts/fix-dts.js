#!/usr/bin/env node
'use strict'
/**
 * Post-build script that duplicates `dist/index.d.ts` into `dist/index.d.cts`
 * and `dist/index.d.mts`.
 *
 * Why: tsup 8.x emits only `index.d.ts` even when configured to emit
 * per-format declarations. Our `exports` map in package.json requires
 * `.d.cts` (for require()) and `.d.mts` (for import). Without these files
 * TypeScript consumers fall back to `any` when they import our package.
 *
 * This script must run AFTER `tsup` finishes (post-build). It's wired into
 * the `build:ts` npm script as `tsup && node scripts/fix-dts.js`.
 */

const { copyFileSync, existsSync, statSync } = require('node:fs')
const { join } = require('node:path')

const DIST = join(__dirname, '..', 'dist')
const SRC = join(DIST, 'index.d.ts')
const D_CTS = join(DIST, 'index.d.cts')
const D_MTS = join(DIST, 'index.d.mts')

if (!existsSync(SRC)) {
  console.error('[fix-dts] ERROR: dist/index.d.ts not found')
  console.error('  Did tsup actually run? Did the DTS build succeed?')
  process.exit(1)
}

const sourceSize = statSync(SRC).size

copyFileSync(SRC, D_CTS)
copyFileSync(SRC, D_MTS)

console.log(`[fix-dts] ✓ copied dist/index.d.ts (${sourceSize} bytes) → .d.cts + .d.mts`)
