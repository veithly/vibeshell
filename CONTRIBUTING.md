# Contributing to VibeShell

[English README](README.md) · [简体中文](README.zh-CN.md) · [日本語](README.ja.md)

## Branches and pull requests

**Open feature, bug-fix, security-fix and documentation PRs against `dev`.** Start your branch from `origin/dev`. The repository default is `dev` so GitHub proposes that base automatically.

`main` is the stable release branch. Its only normal incoming PR is a release promotion from **this repository's `dev` branch**. A fork branch merely named `dev` is not a release promotion. `master` is retained as historical context and is not an active contribution target. There is no separate `develop` branch; `dev` is the development branch.

```bash
git fetch origin
git switch -c fix/short-description origin/dev
# Make and test the change.
git push -u origin fix/short-description
gh pr create --base dev
```

Keep a PR focused, explain user-visible behavior and tradeoffs, and include reproducible test results. The PR target check rejects direct feature PRs to `main`. Retarget an incorrect base rather than deleting the contribution. Release promotions use merge commits to preserve the relationship between `dev`, `main` and version tags.

## Local checks

Use Node.js 22.12+ and current stable Rust, with Tauri's platform prerequisites installed. Do not run arbitrary ignored tests against your saved servers.

```bash
npm ci
cargo run --locked -p vibeshell-plugins --example export_references -- --check
node scripts/check-release.mjs
npm test
npm run build
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

For SSH transport changes, run `bash scripts/test-ssh-compatibility.sh` with Docker available. It creates a loopback-only OpenSSH fixture and generated test keys, not a connection to a production server. Database/credential tests must use injected temporary directories or in-memory stores. Never initialize the user's real database from a unit test.

A successful build is not proof of UI correctness or universal server compatibility. For UI changes, exercise focus, keyboard navigation, reduced motion, resize and unsaved-edit handling. Include sanitized screenshots when useful. Never publish real server details, tokens, private keys or session recordings in fixtures or test logs.

## Plugin and documentation changes

The validated manifests in `plugins/builtin/` define the built-in plugin catalog. Each plugin must expose machine-readable actions and current reference documentation. Details belong in `references/<id>.md`, not in the main Skill.

Regenerate the checked-in references with:

```bash
cargo run --locked -p vibeshell-plugins --example export_references
```

This command writes only the three repository Skill/reference directories. It does not install anything into your home directory or connect to a server. CI checks that generated documents and all three main Skill copies agree. Do not hand-edit generated references; update the manifest or renderer instead.

Update English, Simplified Chinese and Japanese README sections when changing their documented behavior. Preserve command syntax, safety boundaries and language navigation. Additional translations are welcome; do not turn a translation into a separate feature claim.

## Compatibility and security

Preserve exact credential bytes, including meaningful spaces. Secret fields must not enter debug logs or activity summaries. Metadata/credential mutations must commit atomically and roll back on error. Do not weaken host-key verification to make an authentication test pass.

Keep file overwrite, binary encoding, path validation, output limits, cancellation and sync exclusion semantics consistent across transports. Unsupported operations must fail clearly, not report fabricated success. Do not replay a mutating command after a response loss without a server-side idempotency guarantee.

Third-party code must have a compatible license and preserved notices. Contributions are accepted under the repository's GPL-3.0-only terms; by submitting a contribution, you confirm you have the right to provide it on those terms. This is not a copyright assignment. See [NOTICE](NOTICE) for the earlier MIT-licensed portions.

Report exploitable vulnerabilities through the repository's private security reporting channel where available; do not disclose credentials in public issues. Existing dependency advisories are not waived by functional test success.

## Releases

Version changes go through `dev`, then a tested promotion to `main`. Do not bump a version automatically on every branch push. Only an explicit matching `vX.Y.Z` tag on `main` starts publication. See [RELEASING](docs/RELEASING.md).
