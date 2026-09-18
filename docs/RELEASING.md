# Release process

## Branch contract

`dev` is the default branch for all normal PRs. `main` receives only release promotions from this repository's `dev`. `master` is historical. Branch protection requires the PR-target check and CI; force pushes and branch deletion are disallowed on active branches.

A release is an explicit maintainer action, not an automatic side effect of a documentation or source push. There is no auto-increment bot commit.

## Prepare and promote

Update the workspace version, npm manifest and lockfile, the three local Cargo.lock packages, Tauri version, Codex plugin version and Claude marketplace versions in one PR to `dev`. Independent built-in plugin versions are not the application version and should change only when that plugin changes.

Run `node scripts/check-release.mjs`, regenerate/check plugin references, run frontend build/tests, strict Clippy and all workspace tests. Use the loopback SSH fixture for transport changes. Review dependency advisories, licensing and compatibility; do not hide failed scans in release notes.

Open the promotion PR with `gh pr create --base main --head dev`. Wait for required checks. Merge using a merge commit so the tested integration ancestry is preserved. No feature PR goes directly to `main`.

## Tag and publish

```bash
git fetch origin
git switch main
git merge --ff-only origin/main
git tag -a v1.1.0 -m 'VibeShell 1.1.0'
git push origin v1.1.0
```

Use the actual prepared version instead of copying this example for a different release. Tags are immutable: never move a published tag to a different commit. A failed workflow can be rerun for the same tag through **Release → Run workflow**, specifying that existing tag and running the workflow from `main`.

The release workflow validates that the tag resolves to a commit on `main` and its version matches every application manifest. It validates updater signing, creates/retains a draft, builds desktop and native CLI packages for Windows x64, macOS arm64/x64 and Linux x64, and assembles matching source materials. Only after all platforms and source packaging succeed does it publish `latest.json`, checksums and the release.

A failed or partial run must remain a draft. Do not mark it latest or overwrite a previously published release to work around failures. Publishing has no automatic rollback: a regression requires a new version with a tested fix.

## Signatures and source

`TAURI_SIGNING_PRIVATE_KEY` and optional `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` belong in GitHub Actions secrets. They must match the public updater key already shipped to users. Validate cryptographically; matching a comment's key ID is not enough. Never print or replace the key just to make CI pass.

Apple Developer ID signing/notarization requires the separately configured Apple secrets. Without them, macOS artifacts use local ad-hoc resource signatures and the release notes say so. Rebuild and sign updater archives after any modification to the application bundle; a signed old archive is not the repaired application.

Distributions must include LICENSE, NOTICE and legacy attribution. The source bundle combines the exact Git tree with vendored Rust sources and integrity-checked npm package archives from the lockfile. Include the build scripts, dependency manifests and license notices; `SOURCE-README.txt` explains reconstruction. Do not publish private audit records, runtime databases, keys, caches or files outside the tagged source tree.

## Installed-app validation

The GitHub version, a compiled artifact and the installed application are different facts. Do not describe an installed app as updated until the GUI, sidecar and independent CLI match and the new UI has been exercised. Back up existing data consistently and obtain permission before closing active SSH sessions. Never point regression tests at a user's saved-credential store.
