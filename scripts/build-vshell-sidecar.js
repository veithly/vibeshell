import { existsSync, mkdirSync, statSync, copyFileSync, chmodSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync, spawnSync } from 'node:child_process';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(scriptDir, '..');
const isWindows = process.platform === 'win32';
const exeExt = isWindows ? '.exe' : '';

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    stdio: 'inherit',
    shell: isWindows,
    ...options,
  });

  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(' ')} failed with exit code ${result.status ?? 'unknown'}`);
  }
}

function detectTargetTriple() {
  const explicitTarget =
    process.env.TAURI_ENV_TARGET_TRIPLE ||
    process.env.CARGO_BUILD_TARGET ||
    process.env.TARGET ||
    process.env.RUST_TARGET;

  if (explicitTarget) {
    return explicitTarget;
  }

  const rustcInfo = execFileSync('rustc', ['-vV'], {
    cwd: repoRoot,
    encoding: 'utf8',
  });
  const hostLine = rustcInfo.split(/\r?\n/).find((line) => line.startsWith('host:'));
  const host = hostLine?.replace(/^host:\s*/, '').trim();

  if (!host) {
    throw new Error("Could not detect Rust target triple from 'rustc -vV'");
  }

  return host;
}

function assertUsableBinary(path) {
  if (!existsSync(path)) {
    throw new Error(`vshell CLI binary was not produced at ${path}`);
  }

  const size = statSync(path).size;
  if (size === 0) {
    throw new Error(`vshell CLI binary at ${path} is empty`);
  }
}

const targetTriple = detectTargetTriple();
const cargoArgs = ['build', '--release', '--package', 'vshell'];
const hasExplicitTarget = Boolean(
  process.env.TAURI_ENV_TARGET_TRIPLE ||
  process.env.CARGO_BUILD_TARGET ||
  process.env.TARGET ||
  process.env.RUST_TARGET
);

if (hasExplicitTarget) {
  cargoArgs.push('--target', targetTriple);
}

console.log(`[sidecar] Building vshell for ${targetTriple}...`);
run('cargo', cargoArgs, {
  env: {
    ...process.env,
    VIBESHELL_BUILDING_SIDECAR: '1',
  },
});

const targetDir = hasExplicitTarget
  ? join(repoRoot, 'target', targetTriple, 'release')
  : join(repoRoot, 'target', 'release');
const source = join(targetDir, `vshell${exeExt}`);
const binaryDir = join(repoRoot, 'src-tauri', 'binaries');
const destination = join(binaryDir, `vshell-${targetTriple}${exeExt}`);

assertUsableBinary(source);
mkdirSync(binaryDir, { recursive: true });
copyFileSync(source, destination);

if (!isWindows) {
  chmodSync(destination, 0o755);
}

assertUsableBinary(destination);
console.log(`[sidecar] Copied ${source} -> ${destination}`);
