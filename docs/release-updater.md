# VibeShell Updater Signing

VibeShell uses the Tauri 2 updater plugin. Release builds produce signed updater
artifacts and a `latest.json` manifest uploaded to GitHub Releases.

## Signing Key

The updater public key is committed in `src-tauri/tauri.conf.json`.

The matching private key was generated locally at:

```text
~/.tauri/vibeshell-updater.key
```

Keep that file secret. If it is lost, existing installations cannot verify
future updates signed by a different key unless they first install a build that
contains the new public key.

## GitHub Secrets

Set the signing key in the repository secrets:

```bash
gh secret set TAURI_SIGNING_PRIVATE_KEY < ~/.tauri/vibeshell-updater.key
```

This key was generated without a password, so `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
can remain unset. If the key is rotated with a password-protected key, also set:

```bash
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD
```

## Release Output

The release workflow:

1. Builds each desktop platform with `TAURI_SIGNING_PRIVATE_KEY`.
2. Uploads updater artifacts and their `.sig` files to the GitHub Release.
3. Generates `latest.json` with platform URLs and signatures.
4. Uploads `latest.json` to:

```text
https://github.com/veithly/vibeshell/releases/latest/download/latest.json
```

The app checks that endpoint and uses Tauri's `downloadAndInstall()` flow for
one-click signed updates.
