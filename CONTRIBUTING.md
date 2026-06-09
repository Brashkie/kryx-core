# Contributing to `@kryxjs/core`

First off — thanks for considering a contribution. This document explains what we expect and how to make your PR land smoothly.

---

## Code of Conduct

Be kind. Disagreements are about code, not people. Bullying, harassment, or personal attacks are not tolerated. See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

---

## Quick start

```bash
# 1. Fork on GitHub, then:
git clone https://github.com/YOUR_USERNAME/kryx-core.git
cd kryx-core

# 2. Install Rust ≥1.80 (if you haven't):
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 3. Install Node.js ≥18 and the napi-rs CLI globally:
npm install -g @napi-rs/cli

# 4. Install JS deps:
npm install

# 5. Build everything:
npm run build:debug

# 6. Verify everything works:
npm test
```

---

## Project structure

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for a complete tour. Quick reference:

```
src/         → TypeScript source (public API)
crates/      → Rust source
__tests__/   → Vitest test suite
examples/    → Runnable examples
docs/        → Long-form documentation
scripts/     → Build helpers
.github/     → CI workflows
```

---

## What to work on

**Easy wins for new contributors:**
- Bug reports with reproductions
- Documentation improvements (typos, clarifications, examples)
- New examples in `examples/`
- New built-in stages (e.g. throttle, debounce, retry)
- Test coverage for edge cases

**Need a discussion first:**
- New public APIs → [open an issue](https://github.com/Brashkie/kryx-core/issues/new) before coding
- Breaking changes
- Architecture-level decisions
- New dependencies

**Out of scope (will be closed):**
- Codec implementations (belong in `@kryxjs/codecs`)
- Container parsing (belong in `@kryxjs/containers`)
- Streaming protocols (belong in `@kryxjs/stream`)

When in doubt: **ask first**. We'd rather discuss than reject.

---

## Coding standards

### Rust

- `cargo fmt` — enforced by CI
- `cargo clippy --workspace --all-targets -- -D warnings` — zero warnings
- `cargo test --workspace --exclude kc-node` — all tests pass
- Public functions need `///` doc comments
- `unsafe` blocks need a `// SAFETY:` comment explaining invariants
- New types implement `Debug` (auto-derive when possible)

### TypeScript

- `prettier` — enforced
- `eslint` — zero warnings on strict config
- Strict mode (`tsconfig.json` already does)
- Every public symbol has a JSDoc comment
- Prefer `readonly` on properties; `const` over `let`
- Avoid `any`; use `unknown` when you really mean it

### Tests

- Every bug fix → regression test
- Every new feature → unit test + integration test
- TypeScript tests in `__tests__/index.test.ts` (Vitest)
- Rust tests inline via `#[cfg(test)] mod tests { ... }`
- Cross-cutting tests in `crates/kc-core/src/lib.rs` under `smoke_tests`
- Target: **≥95% coverage on changed files**

Run coverage locally:
```bash
npm run test:coverage
npm run coverage:open
```

---

## Commit conventions

We use [Conventional Commits](https://www.conventionalcommits.org). Format:

```
<type>(<optional scope>): <subject>

<optional body>

<optional footer>
```

**Types:**
- `feat:` — new feature
- `fix:` — bug fix
- `docs:` — documentation only
- `test:` — adding/correcting tests
- `refactor:` — code change that isn't a feature or fix
- `perf:` — performance improvement
- `chore:` — tooling, deps, config
- `ci:` — CI changes

**Examples:**
```
feat(pipeline): support AbortSignal for cancellation
fix(buffer): correct slice bounds check
docs(readme): add streaming example
test(timebase): cover overflow path
refactor(error): extract sub-classes
perf(pipeline): avoid allocation in hot path
```

Body: explain the *why*, not the *what*. Reference issues with `Closes #123` or `Fixes #456`.

---

## Pull request checklist

Before opening a PR, verify:

- [ ] `npm test` passes locally
- [ ] `npm run typecheck` passes
- [ ] `npm run lint` passes
- [ ] `npm run clippy` passes
- [ ] `npm run format` and `npm run format:rust` applied
- [ ] New/changed APIs have doc comments
- [ ] New/changed APIs have tests
- [ ] `CHANGELOG.md` updated under `[Unreleased]`
- [ ] If breaking: explained in PR description with migration path

We squash-merge PRs; the PR title becomes the commit message, so make it good.

---

## Review process

1. CI runs automatically (Rust + TS + dual-package + lint).
2. A maintainer reviews within ~3 business days.
3. We may request changes — that's normal; iteration is good.
4. Once approved and CI green, we squash-merge.
5. The change ships in the next release per the [Roadmap](docs/ROADMAP.md).

---

## Releasing (maintainers only)

```bash
# Bump version in package.json AND root Cargo.toml workspace
npm version <major|minor|patch>

# Build and publish
npm run build
npm run preartifacts
napi artifacts
npm publish --access public

# Tag and push
git push --follow-tags
```

CI publishes per-platform binary packages automatically on tag push.

---

## License

By contributing, you agree your contributions are licensed under [Apache-2.0](LICENSE), the same as the project.

---

## Questions?

- 💬 [GitHub Discussions](https://github.com/Brashkie/kryx-core/discussions)
- 🐛 [GitHub Issues](https://github.com/Brashkie/kryx-core/issues)
- 📧 [dev@brashkie.dev](mailto:dev@brashkie.dev)
