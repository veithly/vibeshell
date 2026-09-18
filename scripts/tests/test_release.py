"""Offline tests for release integrity guards; they never use real signing keys."""
import base64
import hashlib
import importlib.util
import json
import subprocess
import tarfile
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parents[1]


def load(name):
    spec = importlib.util.spec_from_file_location(name, SCRIPTS / (name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class ReleaseTests(unittest.TestCase):
    def test_plain_and_encoded_minisign_envelopes(self):
        module = load("verify-updater")
        raw = b"untrusted comment: fixture\nZW1wdHk=\n"
        self.assertEqual(module.minisign_text(raw), raw)
        self.assertEqual(module.minisign_text(base64.b64encode(raw)), raw)
        with self.assertRaises(ValueError):
            module.minisign_text(b"not a signature")

    def test_missing_signature_fails_closed(self):
        module = load("verify-updater")
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(ValueError):
                module.verify(Path(tmp) / "missing", "not-used")

    def test_source_urls_and_integrity_are_checked(self):
        module = load("package-source")
        for url in ["http://registry.npmjs.org/pkg", "https://example.com/pkg", "file:///tmp/pkg"]:
            with self.assertRaises(ValueError):
                module.verified_package(url, "sha512-YWJj")
        data = b"source fixture"
        digest = "sha512-" + base64.b64encode(hashlib.sha512(data).digest()).decode()
        with patch.object(module.urllib.request, "urlopen") as request:
            response = request.return_value.__enter__.return_value
            response.geturl.return_value = "https://registry.npmjs.org/pkg"
            response.read.return_value = data
            self.assertEqual(module.verified_package(response.geturl.return_value, digest), data)
            response.read.return_value = b"changed source"
            with self.assertRaises(ValueError):
                module.verified_package(response.geturl.return_value, digest)

    def test_updater_manifest_requires_every_platform_and_preserves_asset_names(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            command = ['node', str(SCRIPTS / 'generate-latest-json.js'), tmp, '1.1.0', 'v1.1.0', 'owner/repo']
            self.assertNotEqual(subprocess.run(command, capture_output=True).returncode, 0)
            names = ['VibeShell-1.1.0-windows-x64-client-setup.exe',
                     'VibeShell-1.1.0-macos-arm64-VibeShell.app.tar.gz',
                     'VibeShell-1.1.0-macos-x64-VibeShell.app.tar.gz',
                     'VibeShell-1.1.0-linux-x64-client.AppImage']
            for name in names:
                (root / name).write_bytes(b'artifact')
                (root / (name + '.sig')).write_text('fixture-signature')
            subprocess.run(command, capture_output=True, check=True)
            manifest = json.loads((root / 'latest.json').read_text())
            self.assertEqual(manifest['version'], '1.1.0')
            self.assertEqual(set(manifest['platforms']), {'windows-x86_64', 'darwin-aarch64', 'darwin-x86_64', 'linux-x86_64'})
            self.assertEqual({entry['url'].rsplit('/', 1)[1] for entry in manifest['platforms'].values()}, set(names))

    def test_cli_archive_contains_notices_and_skill_references(self):
        module = load('collect-release')
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            files = {'package.json': '{"version":"1.1.0"}', 'LICENSE': 'GPL', 'NOTICE': 'Attribution',
                     'licenses/legacy-MIT.txt': 'MIT', 'skills/vibeshell/SKILL.md': 'Index',
                     'skills/vibeshell/references/test.md': 'Reference', 'cli/README.md': 'CLI',
                     'scripts/install-cli.sh': '#!/bin/sh\n',
                     'target/x86_64-unknown-linux-gnu/release/vibeshell': 'binary',
                     'target/x86_64-unknown-linux-gnu/release/bundle/appimage/app.AppImage': 'app',
                     'target/x86_64-unknown-linux-gnu/release/bundle/appimage/app.AppImage.sig': 'sig'}
            for name, content in files.items():
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(content)
            with patch.object(module, 'ROOT', root), patch('sys.argv', ['collect-release', 'x86_64-unknown-linux-gnu']):
                module.main()
            archive = root / 'target/release-assets/VibeShell-CLI-1.1.0-linux-x64.tar.gz'
            with tarfile.open(archive) as bundled:
                names = bundled.getnames()
                for suffix in ['/LICENSE', '/NOTICE', '/licenses/legacy-MIT.txt', '/skills/vibeshell/references/test.md']:
                    self.assertTrue(any(name.endswith(suffix) for name in names), suffix)

    def test_all_targets_have_distinct_archive_names(self):
        module = load("collect-release")
        self.assertEqual(len(module.TARGETS), 4)
        self.assertEqual(len(set(module.TARGETS.values())), 4)


if __name__ == "__main__":
    unittest.main()
