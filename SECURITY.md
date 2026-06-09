# Security Policy

## Supported versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | ✅ |
| < 0.1   | ❌ |

After v1.0 we will maintain LTS branches for 12 months per major version.

## Reporting a vulnerability

**Please do NOT open public GitHub issues for security vulnerabilities.**

Instead, email **security@brashkie.dev** with:

1. A description of the vulnerability
2. Steps to reproduce (or a minimal proof-of-concept)
3. Impact assessment (what an attacker could do)
4. Your name/handle for credit (optional)

We commit to:

- Acknowledging your report within **48 hours**
- Providing an initial assessment within **5 business days**
- Keeping you informed of progress every **7 days**
- Crediting you in the security advisory (unless you prefer anonymity)

## Disclosure timeline

- **Day 0** — Vulnerability reported privately
- **Day 1-2** — Acknowledgment + triage starts
- **Day 3-30** — Fix developed, tested, reviewed
- **Day 30-90** — Coordinated disclosure with affected downstream packages
- **Day 90** — Public advisory + patch release

We may shorten or extend this timeline based on severity and exploitation status.

## Scope

**In scope:**
- Memory safety issues in the Rust core (`kc-core`, `kc-node`)
- FFI boundary unsoundness
- Privilege escalation via the TypeScript layer
- Supply-chain issues affecting our build

**Out of scope:**
- Bugs in upstream dependencies (report upstream)
- Issues requiring physical access or root privileges
- Theoretical issues without a practical exploit path

## Security features

- `kc-core` is `#![deny(unsafe_op_in_unsafe_fn)]` — every unsafe op is explicit
- All FFI calls have documented `// SAFETY:` invariants
- CI runs `cargo clippy --all-targets -- -D warnings` on every PR
- Dependencies are pinned via `Cargo.lock` and reviewed by Dependabot
- No `eval()`, no `Function(...)`, no dynamic code generation on the TS side

Thank you for helping keep `@kryxjs/core` secure.
