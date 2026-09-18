import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { createHash } from 'node:crypto';
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
const isDevMode = process.argv.includes('--dev');
const aiMemoryVersion = process.env.AI_MEMORY_VERSION || 'v2.2.2';

if (!/^v\d+\.\d+\.\d+$/.test(aiMemoryVersion)) {
  throw new Error(`invalid AI_MEMORY_VERSION: ${aiMemoryVersion}`);
}

function shouldUseShell(command) {
  if (process.platform !== 'win32') {
    return false;
  }

  const normalized = command.toLowerCase();
  return normalized === 'yarn' || normalized.endsWith('.cmd') || normalized.endsWith('.bat');
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    stdio: 'inherit',
    cwd: repoRoot,
    env: {
      ...process.env,
      CARGO_TARGET_DIR: targetDir,
      ...options.env,
    },
    shell: shouldUseShell(command),
    ...options,
  });

  if (result.error) {
    console.error(`[build-sidecars] failed to start ${command}:`, result.error);
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
    if (baseName === 'ai-memory') {
      // Tauri validates externalBin during `tauri dev`, but development does
      // not need a copied runtime. Keep this marker tiny; the real binary is
      // installed separately for dev and downloaded only by release builds.
      writeFileSync(
        placeholder,
        process.platform === 'win32' ? '@echo off\r\nexit /b 1\r\n' : '#!/bin/sh\nexit 1\n',
      );
      chmodSync(placeholder, 0o755);
    } else {
      copyFileSync(process.execPath, placeholder);
    }
  }
}

function copyExecutable(source, target) {
  // Release binaries may be emitted as read-only executables (0555). Make an
  // existing target replaceable so repeated dev/release builds are idempotent,
  // then keep the staged sidecar executable and writable by its owner.
  if (existsSync(target)) {
    chmodSync(target, 0o755);
  }
  copyFileSync(source, target);
  chmodSync(target, 0o755);
}

function copySidecar(baseName, hostTriple) {
  const source = join(releaseDir, binaryName(baseName));
  const target = join(releaseDir, targetBinaryName(baseName, hostTriple));

  if (!existsSync(source)) {
    throw new Error(`missing built sidecar: ${source}`);
  }

  mkdirSync(releaseDir, { recursive: true });
  copyExecutable(source, target);
}

function copyCompiledClaudeSidecar(hostTriple) {
  const baseName = 'dcc-claude-sidecar';
  const source = join(sidecarDistDir, binaryName(baseName));
  const target = join(sidecarDistDir, targetBinaryName(baseName, hostTriple));

  if (!existsSync(source)) {
    throw new Error(`missing built Claude sidecar: ${source}`);
  }

  mkdirSync(sidecarDistDir, { recursive: true });
  copyExecutable(source, target);
}

function aiMemoryAssetFor(hostTriple) {
  const assets = {
    'aarch64-apple-darwin': 'ai-memory-macos-aarch64.tar.gz',
    'x86_64-apple-darwin': 'ai-memory-macos-x86_64.tar.gz',
    'aarch64-unknown-linux-gnu': 'ai-memory-linux-aarch64.tar.gz',
    'aarch64-unknown-linux-musl': 'ai-memory-linux-aarch64.tar.gz',
    'x86_64-unknown-linux-gnu': 'ai-memory-linux-x86_64.tar.gz',
    'x86_64-unknown-linux-musl': 'ai-memory-linux-x86_64.tar.gz',
    'x86_64-pc-windows-msvc': 'ai-memory-windows-x86_64.zip',
  };
  const asset = assets[hostTriple];
  if (!asset) {
    throw new Error(`no ai-memory release asset is mapped for ${hostTriple}`);
  }
  return asset;
}

function findFile(root, expectedName) {
  if (!existsSync(root)) return null;
  for (const entry of readdirSync(root, { withFileTypes: true })) {
    const path = join(root, entry.name);
    if (entry.isDirectory()) {
      const nested = findFile(path, expectedName);
      if (nested) return nested;
    } else if (entry.name === expectedName) {
      return path;
    }
  }
  return null;
}

function sha256(path) {
  return createHash('sha256').update(readFileSync(path)).digest('hex');
}

function stageAiMemory(hostTriple) {
  const asset = aiMemoryAssetFor(hostTriple);
  const cacheDir = join(targetDir, 'ai-memory-cache', aiMemoryVersion);
  const archivePath = join(cacheDir, asset);
  const checksumPath = `${archivePath}.sha256`;
  const extractDir = join(cacheDir, asset.endsWith('.zip') ? 'extract-zip' : 'extract-tar');
  const baseUrl = `https://github.com/akitaonrails/ai-memory/releases/download/${aiMemoryVersion}`;
  mkdirSync(cacheDir, { recursive: true });

  if (!existsSync(archivePath)) {
    run('curl', ['-fsSL', `${baseUrl}/${asset}`, '-o', archivePath]);
  }
  if (!existsSync(checksumPath)) {
    run('curl', ['-fsSL', `${baseUrl}/${asset}.sha256`, '-o', checksumPath]);
  }
  const expected = readFileSync(checksumPath, 'utf8').trim().split(/\s+/)[0].toLowerCase();
  const actual = sha256(archivePath);
  if (!expected || expected !== actual) {
    throw new Error(`ai-memory checksum mismatch for ${asset}: expected ${expected}, got ${actual}`);
  }

  const executableName = binaryName('ai-memory');
  let executable = findFile(extractDir, executableName);
  if (!executable) {
    if (existsSync(extractDir)) rmSync(extractDir, { recursive: true, force: true });
    mkdirSync(extractDir, { recursive: true });
    if (asset.endsWith('.zip')) {
      run('unzip', ['-q', archivePath, '-d', extractDir]);
    } else {
      run('tar', ['-xzf', archivePath, '-C', extractDir]);
    }
    executable = findFile(extractDir, executableName);
  }
  if (!executable) {
    throw new Error(`ai-memory executable was not found inside ${asset}`);
  }

  const staged = join(releaseDir, executableName);
  copyExecutable(executable, staged);
  copySidecar('ai-memory', hostTriple);
  writeFileSync(join(releaseDir, 'ai-memory.version'), `${aiMemoryVersion}\n${actual}\n`);
  console.log(`[build-sidecars] staged ai-memory ${aiMemoryVersion} (${actual})`);
}

const hostTriple = detectHostTriple();
mkdirSync(releaseDir, { recursive: true });
for (const baseName of ['dcc', 'dccd', 'dccd-http', 'ai-memory']) {
  ensurePlaceholder(releaseDir, baseName, hostTriple);
}

if (isDevMode) {
  ensurePlaceholder(sidecarDistDir, 'dcc-claude-sidecar', hostTriple);
  console.log(`[build-sidecars] prepared dev placeholders for ${hostTriple}; ai-memory is expected to be installed separately`);
  process.exit(0);
}

// Compile our SDK integration only. Claude Code is a user-installed prerequisite.
run('bun', ['build', '--compile', 'src/index.mjs', '--outfile', 'dist/dcc-claude-sidecar'], {
  cwd: sidecarDir,
  env: {
    ...process.env,
  },
});
copyCompiledClaudeSidecar(hostTriple);

run('cargo', [
  'build',
  '--manifest-path',
  join(srcTauriDir, 'Cargo.toml'),
  '--release',
  '--bin',
  'dcc',
  '--bin',
  'dccd',
  '--bin',
  'dccd-http',
]);

copySidecar('dcc', hostTriple);
copySidecar('dccd', hostTriple);
copySidecar('dccd-http', hostTriple);
stageAiMemory(hostTriple);

console.log(`[build-sidecars] prepared dcc, dccd, dccd-http, ai-memory and Claude sidecar for ${hostTriple}`);
