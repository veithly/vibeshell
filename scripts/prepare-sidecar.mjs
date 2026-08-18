#!/usr/bin/env node

import { execFileSync, spawnSync } from 'node:child_process';
import { chmodSync, copyFileSync, mkdirSync, statSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(scriptDir, '..');
const args = process.argv.slice(2);

function argumentValue(name) {
  const index = args.indexOf(name);
  if (index >= 0) {
    const value = args[index + 1];
    if (!value || value.startsWith('--')) {
      throw new Error(`${name} requires a value`);
    }
    return value;
  }
  const inline = args.find((argument) => argument.startsWith(`${name}=`));
  return inline?.slice(name.length + 1);
}

function hostTargetTriple() {
  const output = execFileSync('rustc', ['-vV'], {
    cwd: repoRoot,
    encoding: 'utf8',
  });
  const match = /^host:\s+(\S+)$/m.exec(output);
  if (!match) {
    throw new Error('Could not determine the Rust host target from `rustc -vV`');
  }
  return match[1];
}

const target =
  argumentValue('--target') ??
  process.env.VIBESHELL_TARGET ??
  process.env.TAURI_ENV_TARGET_TRIPLE ??
  process.env.CARGO_BUILD_TARGET ??
  hostTargetTriple();

if (target === 'universal-apple-darwin') {
  throw new Error(
    'Build each macOS architecture separately; universal-apple-darwin is not a Cargo target triple.',
  );
}

const profile = args.includes('--debug') ? 'debug' : 'release';
const extension = target.includes('windows') ? '.exe' : '';
const binaryName = `vibeshell${extension}`;
const cargoArgs = [
  'build',
  '--package',
  'vshell',
  '--bin',
  'vibeshell',
  '--target',
  target,
];
if (profile === 'release') {
  cargoArgs.push('--release');
}

if (!args.includes('--no-build')) {
  console.log(`Building native VibeShell CLI for ${target} (${profile})...`);
  const result = spawnSync('cargo', cargoArgs, {
    cwd: repoRoot,
    stdio: 'inherit',
  });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

const source = join(repoRoot, 'target', target, profile, binaryName);
const destinationDir = join(repoRoot, 'src-tauri', 'binaries');
const destination = join(
  destinationDir,
  `vibeshell-${target}${extension}`,
);

mkdirSync(destinationDir, { recursive: true });
copyFileSync(source, destination);
if (!target.includes('windows')) {
  chmodSync(destination, 0o755);
}

const sizeMiB = (statSync(destination).size / 1024 / 1024).toFixed(2);
console.log(`Prepared Tauri sidecar: ${destination} (${sizeMiB} MiB)`);
