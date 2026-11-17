#!/usr/bin/env node
// Deterministic, offline release metadata and documentation checks.
import assert from 'node:assert/strict';
import { readFileSync, existsSync, readdirSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const read = (path) => readFileSync(resolve(root, path), 'utf8');
const json = (path) => JSON.parse(read(path));
const pkg = json('package.json');
const version = pkg.version;
assert.match(version, /^\d+\.\d+\.\d+$/);
const tag = process.argv[2];
if (tag) assert.equal(tag, `v${version}`, 'Tag and source version differ');
assert.equal(pkg.license, 'GPL-3.0-only');
assert.equal(json('package-lock.json').version, version);
assert.equal(json('package-lock.json').packages[''].version, version);
assert.equal(json('src-tauri/tauri.conf.json').version, version);
assert.equal(json('src-tauri/tauri.conf.json').bundle.license, pkg.license);
assert.equal(json('.codex-plugin/plugin.json').version, version);
assert.equal(json('.codex-plugin/plugin.json').license, pkg.license);
const marketplace = json('.claude-plugin/marketplace.json');
assert.equal(marketplace.metadata.version, version);
assert.equal(marketplace.plugins.find((item) => item.name === 'vibeshell').version, version);
const workspace = read('Cargo.toml').split('[workspace.package]')[1]?.split('\n[')[0];
assert.match(workspace ?? '', new RegExp(`version = "${version.replaceAll('.', '\\.')}"`));
assert.match(workspace ?? '', /license = "GPL-3.0-only"/);
for (const path of ['src-tauri/Cargo.toml', 'cli/Cargo.toml', 'plugins/Cargo.toml']) {
  assert.match(read(path), /license\.workspace = true/);
}
const lock = read('Cargo.lock');
for (const name of ['vibeshell-desktop', 'vibeshell-plugins', 'vshell']) {
  const block = lock.split('[[package]]').find((entry) => entry.includes(`name = "${name}"`));
  assert.match(block ?? '', new RegExp(`version = "${version.replaceAll('.', '\\.')}"`), `Cargo.lock ${name}`);
}
assert.match(read('LICENSE'), /GNU GENERAL PUBLIC LICENSE\s+Version 3, 29 June 2007/);
assert.match(read('licenses/legacy-MIT.txt'), /MIT License/);
assert.match(read('NOTICE'), /GPL-3.0-only/);
assert(!pkg.dependencies?.gsap && !pkg.dependencies?.['@gsap/react']);
const canonical = read('skills/vibeshell/SKILL.md');
const ids = readdirSync(resolve(root, 'plugins/builtin'), { withFileTypes: true })
  .filter((entry) => entry.isDirectory()).map((entry) => entry.name).sort();
for (const directory of ['skills/vibeshell', '.claude/skills/vibeshell', '.codex/skills/vibeshell']) {
  assert.equal(read(`${directory}/SKILL.md`), canonical, `Skill drift: ${directory}`);
  const actual = readdirSync(resolve(root, directory, 'references')).filter((name) => name.endsWith('.md')).map((name) => name.slice(0, -3)).sort();
  assert.deepEqual(actual, ids, `Reference catalog drift: ${directory}`);
  for (const id of ids) assert(canonical.includes(`references/${id}.md`));
}
for (const path of ['README.md', 'README.zh-CN.md', 'README.ja.md', 'CONTRIBUTING.md', 'docs/RELEASING.md', 'docs/AGENT_COLLABORATION.md']) {
  const text = read(path);
  for (const match of text.matchAll(/\]\(([^\s)]+)\)/g)) {
    const target = match[1].split('#')[0];
    if (!target || /^[a-z][a-z0-9+.-]*:/i.test(target)) continue;
    assert(existsSync(resolve(root, dirname(path), target)), `${path}: missing ${target}`);
  }
}
console.log(`Release metadata, license, README links and ${ids.length} plugin references agree at ${version}.`);
