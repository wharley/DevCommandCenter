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

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    stdio: 'inherit',
    cwd: repoRoot,
    env: {
      ...process.env,
      CARGO_TARGET_DIR: targetDir,
      ...options.env,
    },
    ...options,
  });

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

function copySidecar(baseName, hostTriple) {
  const source = join(releaseDir, binaryName(baseName));
  const target = join(releaseDir, `${baseName}-${hostTriple}${process.platform === 'win32' ? '.exe' : ''}`);

  if (!existsSync(source)) {
    throw new Error(`missing built sidecar: ${source}`);
  }

  mkdirSync(releaseDir, { recursive: true });
  copyFileSync(source, target);
}

function copyCompiledClaudeSidecar(hostTriple) {
  const baseName = 'dcc-claude-sidecar';
  const source = join(sidecarDistDir, binaryName(baseName));
  const target = join(
    sidecarDistDir,
    `${baseName}-${hostTriple}${process.platform === 'win32' ? '.exe' : ''}`,
  );

  if (!existsSync(source)) {
    throw new Error(`missing built Claude sidecar: ${source}`);
  }

  mkdirSync(sidecarDistDir, { recursive: true });
  copyFileSync(source, target);
}

const hostTriple = detectHostTriple();
mkdirSync(releaseDir, { recursive: true });
for (const baseName of ['dcc', 'dccd']) {
  const placeholder = join(releaseDir, `${baseName}-${hostTriple}${process.platform === 'win32' ? '.exe' : ''}`);
  if (!existsSync(placeholder)) {
    copyFileSync(process.execPath, placeholder);
  }
}

run('cargo', ['build', '--manifest-path', join(srcTauriDir, 'Cargo.toml'), '--release', '--bins']);

run('yarn', ['build'], {
  cwd: sidecarDir,
  env: {
    ...process.env,
  },
});
copyCompiledClaudeSidecar(hostTriple);

copySidecar('dcc', hostTriple);
copySidecar('dccd', hostTriple);

console.log(`[build-sidecars] prepared dcc, dccd and Claude sidecar for ${hostTriple}`);
