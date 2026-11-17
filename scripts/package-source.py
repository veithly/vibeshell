#!/usr/bin/env python3
"""Package the committed tree and locked dependency sources; never copy the working tree."""
import argparse
import base64
import concurrent.futures
import hashlib
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile
import urllib.request
import urllib.parse

ROOT = Path(__file__).resolve().parent.parent
MAX_PACKAGE_BYTES = 200 * 1024 * 1024


def verified_package(url, integrity):
    parsed = urllib.parse.urlsplit(url)
    if parsed.scheme != "https" or parsed.hostname != "registry.npmjs.org" or parsed.username:
        raise ValueError("Source package must use the public npm HTTPS registry")
    supported = [part.split("-", 1) for part in integrity.split() if part.startswith(("sha512-", "sha256-"))]
    if not supported:
        raise ValueError("A strong lockfile integrity digest is required")
    algorithm, expected = next((pair for pair in supported if pair[0] == "sha512"), supported[0])
    with urllib.request.urlopen(url, timeout=90) as response:
        if urllib.parse.urlsplit(response.geturl()).hostname != "registry.npmjs.org":
            raise ValueError("Unexpected source package redirect")
        data = response.read(MAX_PACKAGE_BYTES + 1)
    if len(data) > MAX_PACKAGE_BYTES:
        raise ValueError("Source package exceeds download bound")
    if hashlib.new(algorithm, data).digest() != base64.b64decode(expected, validate=True):
        raise ValueError("Source package integrity mismatch")
    return data


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "target/release-assets")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    manifest = json.loads(subprocess.check_output(["git", "show", "HEAD:package.json"], cwd=ROOT))
    version = manifest["version"]
    with tempfile.TemporaryDirectory(prefix="vibeshell-source-") as tmp:
        tmp = Path(tmp)
        tree_tar = tmp / "tree.tar"
        subprocess.run(["git", "archive", "--format=tar", "-o", str(tree_tar), "HEAD"], cwd=ROOT, check=True)
        source = tmp / f"VibeShell-Source-{version}"
        source.mkdir()
        with tarfile.open(tree_tar) as archive:
            archive.extractall(source, filter="data")
        config = subprocess.check_output(["cargo", "vendor", "--locked", "--versioned-dirs", "vendor/cargo"], cwd=source, text=True)
        (source / ".cargo").mkdir(exist_ok=True)
        (source / ".cargo/vendor-config.toml").write_text(config, encoding="utf-8")
        lock = json.loads((source / "package-lock.json").read_text(encoding="utf-8"))
        npm_dir = source / "vendor/npm"
        npm_dir.mkdir(parents=True)
        packages = {}
        for entry in lock["packages"].values():
            if "resolved" in entry:
                url = entry["resolved"]
                integrity = entry.get("integrity", "")
                if url in packages and packages[url] != integrity:
                    raise ValueError("Conflicting lockfile integrity for one URL")
                packages[url] = integrity
        def fetch(item):
            url, integrity = item
            name = hashlib.sha256(url.encode()).hexdigest() + ".tgz"
            (npm_dir / name).write_bytes(verified_package(url, integrity))
            return url, name
        with concurrent.futures.ThreadPoolExecutor(max_workers=6) as pool:
            local = dict(pool.map(fetch, packages.items()))
        (npm_dir / "sources.json").write_text(json.dumps({url: {"file": name, "integrity": packages[url]} for url, name in local.items()}, indent=2) + "\n", encoding="utf-8")
        for entry in lock["packages"].values():
            if "resolved" in entry:
                entry["resolved"] = "file:vendor/npm/" + local[entry["resolved"]]
        (source / "package-lock.offline.json").write_text(json.dumps(lock, indent=2) + "\n", encoding="utf-8")
        (source / "SOURCE-README.txt").write_text(f"""VibeShell {version} — corresponding source materials
Git revision: {revision}

This bundle contains the committed source tree, build/release scripts, Cargo.lock,
package-lock.json, Rust crate sources under vendor/cargo, and integrity-checked
npm distribution source archives under vendor/npm. Each component retains its
own license notices. Toolchains and operating-system system libraries are not
included. General-purpose compiler/bundler tools may include platform binaries.

Read README.md and docs/RELEASING.md for the platform prerequisites and commands.
For vendored Rust: cargo --config .cargo/vendor-config.toml build --offline --locked
  --release -p vshell --bin vibeshell
For npm without fetching registry archives: keep the original lockfile, copy
package-lock.offline.json to package-lock.json in your extracted build directory,
then run npm ci --offline. This alternate lock changes only resolved archive
locations; the original lock remains in this bundle. Native install scripts or
platform prerequisites can require separately installed tools.

No publisher signing key is needed to compile, modify, or locally install this
source. Disable updater-artifact generation in a local Tauri config override or
use your own updater signing key/public key for your own distribution. Never
replace a running user's application or close SSH sessions as part of a build.

VibeShell as a whole: GPL-3.0-only. Read LICENSE, NOTICE and licenses/legacy-MIT.txt.
""", encoding="utf-8")
        destination = args.output / f"VibeShell-Source-{version}.tar.gz"
        with tarfile.open(destination, "w:gz") as archive:
            archive.add(source, arcname=source.name)
        if destination.stat().st_size >= 2_000_000_000:
            raise ValueError("Source asset exceeds the release asset size limit")
        print(f"Packaged revision {revision}: {destination} ({len(local)} npm sources)")


if __name__ == "__main__":
    main()
