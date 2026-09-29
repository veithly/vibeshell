# Release process

## Branch contract

`dev` is the default branch for all normal PRs. `main` receives only release promotions from this repository's `dev`. `master` is historical. Branch protection still requires PR Target, Frontend Check, Clippy Lint and all three Rust Check jobs; force pushes and branch deletion are disallowed on active branches.

**Both CI compilation and release packaging/publication are manual-only.** A push, PR or version tag starts neither workflow. The metadata-only PR Target workflow remains automatic. There is no auto-increment bot commit, and no workflow automatically dispatches another workflow.

Manual CI uses two small reporting jobs to bridge GitHub's PR-check event restriction: initialize the existing required commit statuses as pending, then publish results from actual jobs on the same SHA and run attempt. Builds have no status-write permission. Missing/skipped/failed evidence cannot become green, and superseded runs stop reporting. Branch protection still requires all five CI contexts from GitHub Actions plus PR Target; there is no administrator bypass. See [GitHub's required-check documentation](https://docs.github.com/en/pull-requests/how-tos/merge-and-close-pull-requests/troubleshooting-required-status-checks#checks-from-some-workflow-jobs-are-not-evaluated).

## Prepare and promote

Update the workspace version, npm manifest and lockfile, the three local Cargo.lock packages, Tauri version, Codex plugin version and Claude marketplace versions in one PR to `dev`. Independent built-in plugin versions are not the application version and should change only when that plugin changes.

Run `node scripts/check-release.mjs`, `node --test scripts/tests/*.test.mjs`, regenerate/check plugin references, run frontend build/tests, strict Clippy and all workspace tests. Use the loopback SSH fixture for transport changes. Review dependency advisories, licensing and compatibility; do not hide failed scans in release notes.

After reviewing a normal PR, run **Actions → CI → Run workflow**, selecting its head branch, or `gh workflow run ci.yml --ref <head-branch>`. New commits require a fresh manual run. Required checks are not waived just because there is no automatic trigger. See [CONTRIBUTING](../CONTRIBUTING.md#manual-github-checks) for fork PRs.

Open the promotion PR with `gh pr create --base main --head dev`, then explicitly run `gh workflow run ci.yml --ref dev`. Wait for all required checks and merge using a merge commit so integration ancestry is preserved. No feature PR goes directly to `main`. Because that merge creates a new commit, its release validation must run on the resulting `main` commit, not merely on the earlier PR head.

## Tag, verify and build a draft

```bash
git fetch origin
git switch main
git merge --ff-only origin/main
# The version must already have been prepared and promoted above.
TAG="v$(node -p 'require("./package.json").version')"
git tag -a "$TAG" -m "VibeShell ${TAG#v}"
git push origin "$TAG"

# Pushing the tag does nothing else. Explicitly test that exact commit:
gh workflow run ci.yml --ref "$TAG"
```

Never move or recreate an existing tag. After the tag's manual CI run has succeeded, run **Actions → Release → Run workflow**, select branch **main**, enter the existing tag and leave **publish** unchecked. The CLI equivalent is:

```bash
gh workflow run release.yml --ref main -f tag="$TAG" -f publish=false
```

The workflow only accepts dispatches from `main`. It validates that the tag resolves to a commit on `main` and that its version matches every application manifest. The latest manual CI run for that exact commit must have succeeded in this repository's CI workflow, including frontend, Clippy and Linux/Windows/macOS Rust jobs. An old successful check, an automatic run, a skipped platform or a newer failed attempt is insufficient. CI run/job validation uses the current gate from `main`, even when a selected tag predates that helper.

Release validates updater signing, creates/retains a draft, builds desktop and native CLI packages for Windows x64, macOS arm64/x64 and Linux x64, and assembles matching source materials. Only after every platform, source bundle, signature and checksum check succeeds are the complete assets uploaded. With the default `publish=false`, the release stays a draft and is not marked latest.

## Publish explicitly

To authorize publication, explicitly select **publish** in a manual Release run from `main`, or:

```bash
gh workflow run release.yml --ref main -f tag="$TAG" -f publish=true
```

This is a full verified build-and-publish run, not a shortcut that publishes unchecked files from an earlier draft. A separate draft build is optional; a maintainer may choose `publish=true` on the first authorized run. Pushing a tag, running CI, or completing a draft build never publishes on its own.

A failed or partial run must remain a draft. Do not mark it latest or overwrite a previously published release to work around failures. Publishing has no automatic rollback: a regression requires a new version with a tested fix.

## Signatures and source

`TAURI_SIGNING_PRIVATE_KEY` and optional `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` belong in GitHub Actions secrets. They must match the public updater key already shipped to users. Validate cryptographically; matching a comment's key ID is not enough. Never print or replace the key just to make CI pass.

Apple Developer ID signing/notarization requires the separately configured Apple secrets. Without them, macOS artifacts use local ad-hoc resource signatures and the release notes say so. Rebuild and sign updater archives after any modification to the application bundle; a signed old archive is not the repaired application.

Distributions must include LICENSE, NOTICE and legacy attribution. The source bundle combines the exact Git tree with vendored Rust sources and integrity-checked npm package archives from the lockfile. Include the build scripts, dependency manifests and license notices; `SOURCE-README.txt` explains reconstruction. Do not publish private audit records, runtime databases, keys, caches or files outside the tagged source tree.

## Installed-app validation

The GitHub version, a compiled artifact and the installed application are different facts. Do not describe an installed app as updated until the GUI, sidecar and independent CLI match and the new UI has been exercised. Back up existing data consistently and obtain permission before closing active SSH sessions. Never point regression tests at a user's saved-credential store.
