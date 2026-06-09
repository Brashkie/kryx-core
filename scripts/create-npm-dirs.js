'use strict'
/**
 * Creates the per-platform npm package directories AND moves the .node binaries
 * from the workflow artifacts into each one.
 *
 * Runs in two phases:
 *   1. Always: writes package.json files into npm/<platform>/
 *   2. If `artifacts/` exists: moves .node binaries from artifacts/bindings-<target>/
 *      into npm/<platform>/, renaming to kryx-core.node
 *
 * Triggered by the `preartifacts` npm script (before `napi artifacts`).
 */

const { mkdirSync, writeFileSync, readdirSync, copyFileSync, existsSync, statSync } = require('fs')
const { join } = require('path')

const ROOT = join(__dirname, '..')
const PKG = require(join(ROOT, 'package.json'))

// platform name → rust target triple (used to find the artifact folder)
const PLATFORMS = [
  { name: 'win32-x64-msvc',   triple: 'x86_64-pc-windows-msvc',     os: 'win32',  cpu: 'x64',   libc: null    },
  { name: 'win32-arm64-msvc', triple: 'aarch64-pc-windows-msvc',    os: 'win32',  cpu: 'arm64', libc: null    },
  { name: 'darwin-x64',       triple: 'x86_64-apple-darwin',        os: 'darwin', cpu: 'x64',   libc: null    },
  { name: 'darwin-arm64',     triple: 'aarch64-apple-darwin',       os: 'darwin', cpu: 'arm64', libc: null    },
  { name: 'linux-x64-gnu',    triple: 'x86_64-unknown-linux-gnu',   os: 'linux',  cpu: 'x64',   libc: 'glibc' },
  { name: 'linux-x64-musl',   triple: 'x86_64-unknown-linux-musl',  os: 'linux',  cpu: 'x64',   libc: 'musl'  },
  { name: 'linux-arm64-gnu',  triple: 'aarch64-unknown-linux-gnu',  os: 'linux',  cpu: 'arm64', libc: 'glibc' },
]

const ARTIFACTS_DIR = join(ROOT, 'artifacts')
const hasArtifacts = existsSync(ARTIFACTS_DIR)

if (hasArtifacts) {
  console.log('📦 artifacts/ found — will move .node binaries\n')
} else {
  console.log('ℹ artifacts/ not present — only generating package.json files\n')
}

let moved = 0

for (const platform of PLATFORMS) {
  const dir = join(ROOT, 'npm', platform.name)
  mkdirSync(dir, { recursive: true })

  // ── 1. Write platform package.json ─────────────────────────────────────
  const platformPkg = {
    name: `@kryxjs/core-${platform.name}`,
    version: PKG.version,
    description: `${PKG.description} (${platform.name})`,
    os: [platform.os],
    cpu: [platform.cpu],
    ...(platform.libc ? { libc: [platform.libc] } : {}),
    main: 'kryx-core.node',
    files: ['kryx-core.node'],
    license: PKG.license,
    repository: PKG.repository,
  }

  writeFileSync(
    join(dir, 'package.json'),
    JSON.stringify(platformPkg, null, 2) + '\n',
  )
  console.log(`✓ npm/${platform.name}/package.json`)

  // ── 2. Move the .node binary if available ──────────────────────────────
  if (!hasArtifacts) continue

  // GitHub Actions downloads each artifact as artifacts/bindings-<target>/
  const artifactDir = join(ARTIFACTS_DIR, `bindings-${platform.triple}`)
  if (!existsSync(artifactDir)) {
    console.log(`  ⚠ no artifact dir: ${artifactDir} — skipping binary`)
    continue
  }

  // The .node may be at the root or inside a sub-folder. Search for it.
  const nodeFile = findNodeBinary(artifactDir)
  if (!nodeFile) {
    console.log(`  ⚠ no .node found in ${artifactDir} — skipping binary`)
    continue
  }

  const dest = join(dir, 'kryx-core.node')
  copyFileSync(nodeFile, dest)
  console.log(`  → copied ${nodeFile} → ${dest}`)
  moved += 1
}

if (hasArtifacts) {
  console.log(`\n✓ ${moved}/${PLATFORMS.length} binaries placed into npm/<platform>/`)
} else {
  console.log('\n✓ All platform package.json files created')
}

// ─── helpers ─────────────────────────────────────────────────────────────────

function findNodeBinary(dir) {
  const entries = readdirSync(dir)
  // Prefer files at the top level
  for (const entry of entries) {
    if (entry.endsWith('.node')) return join(dir, entry)
  }
  // Recurse one level (artifact-action sometimes nests)
  for (const entry of entries) {
    const full = join(dir, entry)
    if (statSync(full).isDirectory()) {
      const inner = findNodeBinary(full)
      if (inner) return inner
    }
  }
  return null
}
