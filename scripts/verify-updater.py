#!/usr/bin/env python3
"""Cryptographically verify Tauri updater signatures with the shipped public key."""
import argparse
import base64
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent


def minisign_text(raw):
    raw = raw.strip()
    if not raw.startswith(b"untrusted comment:"):
        raw = base64.b64decode(raw, validate=True)
    if not raw.startswith(b"untrusted comment:"):
        raise ValueError("Invalid minisign envelope")
    return raw + (b"" if raw.endswith(b"\n") else b"\n")


def verify(path, public_key):
    signature = Path(str(path) + ".sig")
    if not path.is_file() or not signature.is_file():
        raise ValueError(f"Missing signed artifact: {path.name}")
    with tempfile.TemporaryDirectory(prefix="vibeshell-verify-") as tmp:
        key = Path(tmp) / "public.key"
        sig = Path(tmp) / "artifact.sig"
        key.write_bytes(minisign_text(public_key.encode()))
        sig.write_bytes(minisign_text(signature.read_bytes()))
        subprocess.run(["minisign", "-Vm", str(path), "-p", str(key), "-x", str(sig)], check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("path", type=Path)
    args = parser.parse_args()
    public_key = json.loads((ROOT / "src-tauri/tauri.conf.json").read_text())["plugins"]["updater"]["pubkey"]
    artifacts = [args.path] if args.path.is_file() else [Path(str(path)[:-4]) for path in args.path.glob("*.sig")]
    if not artifacts:
        raise ValueError("No updater signatures to verify")
    for artifact in artifacts:
        verify(artifact, public_key)
    print(f"Verified {len(artifacts)} updater signatures against the shipped key")


if __name__ == "__main__":
    main()
