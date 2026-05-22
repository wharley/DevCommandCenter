import { copyFileSync, existsSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(scriptDir, '..');
const sidecarDir = join(repoRoot, 'sidecar');
const srcTauriDir = join(repoRoot, 'src-tauri');
const targetDir = join(srcTauriDir, 'target');
const releaseDir = join(targetDir, 'release');
const sidecarDistDir = join(sidecarDir, 'dist');

function resolveCommand(command) {
  if (process.platform !== 'win32') {
    return command;
  }

  if (command === 'yarn') {
    return 'yarn.cmd';
  }

  return command;
}

function run(command, args, options = {}) {
  const resolvedCommand = resolveCommand(command);
  const result = spawnSync(resolvedCommand, args, {
    stdio: 'inherit',
    cwd: repoRoot,
    env: {
      ...process.env,
      CARGO_TARGET_DIR: targetDir,
      ...options.env,
    },
    ...options,
  });

  if (result.error) {
    console.error(`[build-sidecars] failed to start ${resolvedCommand}:`, result.error);
    process.exit(1);
  }

  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

function detectHostTriple() {
  const result = spawnSync('rustc', ['-vV'], {
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'inherit'],
  });

  if (result.status !== 0 || !result.stdout) {
    throw new Error('failed to detect rust host triple');
  }

  const hostLine = result.stdout
    .split('\n')
    .find((line) => line.startsWith('host: '));

  if (!hostLine) {
    throw new Error('rust host triple not found');
  }

  return hostLine.slice('host: '.length).trim();
}

function binaryName(baseName) {
  return process.platform === 'win32' ? `${baseName}.exe` : baseName;
}

function targetBinaryName(baseName, hostTriple) {
  return `${baseName}-${hostTriple}${process.platform === 'win32' ? '.exe' : ''}`;
}

function ensurePlaceholder(dir, baseName, hostTriple) {
  const placeholder = join(dir, targetBinaryName(baseName, hostTriple));
  if (!existsSync(placeholder)) {
    mkdirSync(dir, { recursive: true });
    copyFileSync(process.execPath, placeholder);
  }
}

function copySidecar(baseName, hostTriple) {
  const source = join(releaseDir, binaryName(baseName));
  const target = join(releaseDir, targetBinaryName(baseName, hostTriple));

  if (!existsSync(source)) {
    throw new Error(`missing built sidecar: ${source}`);
  }

  mkdirSync(releaseDir, { recursive: true });
  copyFileSync(source, target);
}

function copyCompiledClaudeSidecar(hostTriple) {
  const baseName = 'dcc-claude-sidecar';
  const source = join(sidecarDistDir, binaryName(baseName));
  const target = join(sidecarDistDir, targetBinaryName(baseName, hostTriple));

  if (!existsSync(source)) {
    throw new Error(`missing built Claude sidecar: ${source}`);
  }

  mkdirSync(sidecarDistDir, { recursive: true });
  copyFileSync(source, target);
}

const hostTriple = detectHostTriple();
mkdirSync(releaseDir, { recursive: true });
for (const baseName of ['dcc', 'dccd']) {
  ensurePlaceholder(releaseDir, baseName, hostTriple);
}

// Sidecar `yarn build` must run before `cargo build --bins`: the Tauri crate's build
// script validates `bundle.resources` (e.g. `../sidecar/dist/vendor`) while compiling
// `dev-command-center-tauri`, and `stage-vendor.mjs` only runs during this step.
run('yarn', ['build'], {
  cwd: sidecarDir,
  env: {
    ...process.env,
  },
});
copyCompiledClaudeSidecar(hostTriple);

run('cargo', ['build', '--manifest-path', join(srcTauriDir, 'Cargo.toml'), '--release', '--bins']);

copySidecar('dcc', hostTriple);
copySidecar('dccd', hostTriple);

console.log(`[build-sidecars] prepared dcc, dccd and Claude sidecar for ${hostTriple}`);
