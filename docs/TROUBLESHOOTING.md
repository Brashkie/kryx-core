# Troubleshooting

Common problems when building, installing, or running `@kryxjs/core`.

---

## Installation problems

### `Cannot find module '@kryxjs/core-linux-x64-gnu'`

Your platform's native binary failed to install. Common causes:

1. **No prebuilt binary for your platform** — check the [supported platforms](../README.md#installation). If unsupported, build from source (see below).
2. **`optionalDependencies` were skipped** — some npm flags (`--no-optional`, `--ignore-optional`) prevent platform packages from installing. Reinstall without them:
   ```bash
   rm -rf node_modules package-lock.json
   npm install
   ```
3. **musl vs glibc mismatch on Linux** — Alpine and other musl-based distros need the `musl` variant. Verify:
   ```bash
   ldd --version    # glibc reports the version, musl errors out
   ```

### Building from source instead of prebuilt

If a prebuilt binary doesn't work for you:

```bash
# Make sure Rust is installed:
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Then in your project:
npm install @kryxjs/core --build-from-source
```

### `napi: command not found`

Install the CLI globally:

```bash
npm install -g @napi-rs/cli
```

---

## Build problems

### `error: cannot find -lstdc++` on Linux

Install build tools:

```bash
# Debian / Ubuntu
sudo apt-get install -y build-essential

# Alpine
apk add --no-cache build-base

# Fedora / RHEL
sudo dnf groupinstall "Development Tools"
```

### `error: linking with 'cc' failed` on macOS

Install Xcode Command Line Tools:

```bash
xcode-select --install
```

### Compilation takes forever

First builds are slow (15-20 min for release) because Rust compiles every dependency from source. Subsequent builds use the cache and are seconds.

To speed up:

- Use `npm run build:debug` during development (much faster, slightly slower runtime).
- Enable `sccache` for distributed caching: `cargo install sccache && export RUSTC_WRAPPER=sccache`.

### `warning: hard linking files in the incremental compilation cache failed` (Windows)

Harmless. Happens when the `target/` dir is on a drive that doesn't support hard links well (UNC paths). We've already disabled incremental compilation for `npm run test:rust` to avoid it.

If you see it elsewhere:

```powershell
$env:CARGO_INCREMENTAL = "0"
```

---

## Runtime problems

### `MediaError: pipeline has no stages`

You called `Pipeline.builder().build()` without adding any stages. Add at least one:

```ts
Pipeline.builder().stage(PassthroughStage).build()
```

### `MediaError: pipeline "X" is stopped`

You tried to `process()` after calling `stop()`. Pipelines are not reusable; create a new one:

```ts
await pipeline.stop()
const fresh = Pipeline.builder().stage(...).build()
```

### `MediaError: stage "X" failed during process: ...`

A stage threw. The `cause` property holds the original error:

```ts
try {
  await pipeline.process(buf)
} catch (err) {
  if (MediaError.is(err)) {
    console.error('stage:', err.context)
    console.error('cause:', err.cause)
  }
}
```

### Pipeline doesn't process anything

Common causes:

1. **Forgot to `await`** — `pipeline.process()` returns a Promise.
2. **All stages return `[]`** — check that no `DropStage` is in the chain.
3. **AbortSignal is already aborted** — check `signal.aborted` before calling.

### Memory keeps growing

Frames clone cheaply but if you hold references, they don't free. Check:

- You're not pushing frames into an unbounded array.
- Long-lived `CounterStage` or custom stateful stages aren't holding frames.
- The `MediaBufferMut` you opened is being `freeze()`d, not just leaked.

---

## Test problems

### Vitest tests fail with "Given napi value is not an array"

The native addon was loaded instead of the mock. Check:

- `__tests__/setup.ts` exists
- `vitest.config.ts` has `setupFiles: ['__tests__/setup.ts']`

### `cargo test` times out

The first compile takes a long time. After:

```bash
cargo test --workspace --exclude kc-node
```

subsequent runs use the cache.

### Coverage shows 0% on a file

The file isn't being imported in any test. Add at least a trivial import:

```ts
import '../src/your-file'
```

---

## Publishing problems

### `npm publish` says version exists

Bump the version first:

```bash
npm version patch  # or minor/major
npm run build
npm publish --access public
```

### Per-platform packages out of sync

The version in each `npm/<platform>/package.json` must match the root `package.json`. Run `npm run preartifacts` to regenerate them.

---

## Still stuck?

- 💬 [Discussions](https://github.com/Brashkie/kryx-core/discussions)
- 🐛 [Issues](https://github.com/Brashkie/kryx-core/issues)
- 📧 [dev@brashkie.dev](mailto:dev@brashkie.dev)

Include in your report:
- `node --version`
- `rustc --version`
- Operating system + architecture
- Full command output (not just the last line)
