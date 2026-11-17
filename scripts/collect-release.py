#!/usr/bin/env python3
"""Collect one target's built artifacts and a native CLI archive with notices."""
import argparse
import json
from pathlib import Path
import shutil
import tarfile
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parent.parent
TARGETS = {
    "aarch64-apple-darwin": "macos-arm64",
    "x86_64-apple-darwin": "macos-x64",
    "x86_64-pc-windows-msvc": "windows-x64",
    "x86_64-unknown-linux-gnu": "linux-x64",
}
EXTENSIONS = (".dmg", ".exe", ".msi", ".AppImage", ".deb", ".rpm", ".tar.gz", ".zip", ".sig")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("target", choices=TARGETS)
    parser.add_argument("--output", type=Path, default=ROOT / "target/release-assets")
    args = parser.parse_args()
    version = json.loads((ROOT / "package.json").read_text())["version"]
    slug = TARGETS[args.target]
    output = args.output
    output.mkdir(parents=True, exist_ok=True)
    build = ROOT / "target" / args.target / "release"
    artifacts = [path for path in (build / "bundle").rglob("*") if path.is_file() and path.name.endswith(EXTENSIONS) and not any(part.endswith(".app") for part in path.parts)]
    if not artifacts or not any(path.name.endswith(".sig") for path in artifacts):
        raise RuntimeError("Desktop bundles and updater signatures are required")
    for path in artifacts:
        destination = output / f"VibeShell-{version}-{slug}-{path.name}"
        if destination.exists():
            raise RuntimeError(f"Duplicate artifact name: {destination.name}")
        shutil.copy2(path, destination)
    binary = build / ("vibeshell.exe" if "windows" in slug else "vibeshell")
    if not binary.is_file():
        raise RuntimeError("Native CLI binary is missing")
    with tempfile.TemporaryDirectory(prefix="vibeshell-cli-") as tmp:
        directory = Path(tmp) / f"VibeShell-CLI-{version}-{slug}"
        directory.mkdir()
        shutil.copy2(binary, directory / binary.name)
        for name in ("LICENSE", "NOTICE"):
            shutil.copy2(ROOT / name, directory / name)
        shutil.copytree(ROOT / "licenses", directory / "licenses")
        shutil.copytree(ROOT / "skills/vibeshell", directory / "skills/vibeshell")
        shutil.copy2(ROOT / "cli/README.md", directory / "README.md")
        if "windows" in slug:
            shutil.copy2(ROOT / "scripts/install-cli.ps1", directory / "install.ps1")
            with zipfile.ZipFile(output / f"{directory.name}.zip", "w", zipfile.ZIP_DEFLATED) as archive:
                for path in directory.rglob("*"):
                    if path.is_file():
                        archive.write(path, path.relative_to(directory.parent))
        else:
            script = directory / "install.sh"
            shutil.copy2(ROOT / "scripts/install-cli.sh", script)
            script.chmod(0o755)
            (directory / binary.name).chmod(0o755)
            with tarfile.open(output / f"{directory.name}.tar.gz", "w:gz") as archive:
                archive.add(directory, arcname=directory.name)
    print(f"Collected {len(artifacts)} desktop files and one licensed CLI archive for {slug}")


if __name__ == "__main__":
    main()
