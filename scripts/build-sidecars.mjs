#!/usr/bin/env node
/**
 * The helpers the app bundle carries beside it (`scripts/native-local.mjs`
 * and the release copy them into Contents/MacOS, or /usr/lib on Linux):
 * the CLI and the daemons from `dcc-server` (`dcc`, `dccd`, `dccd-http`),
 * the pinned ai-memory release (checksum checked), the Claude SDK
 * helper compiled with bun and, on macOS, serve-sim's native addon for the
 * iOS Simulator (pinned npm release, checksum checked; the Claude helper
 * loads it, see sidecar/src/device.mjs). All land in target/release under
 * their plain names; the Claude helper in sidecar/dist.
 */
import { chmodSync, copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(scriptDir, '..');
const sidecarDir = join(repoRoot, 'sidecar');
const targetDir = join(repoRoot, 'target');
const releaseDir = join(targetDir, 'release');
const aiMemoryVersion = process.env.AI_MEMORY_VERSION || 'v2.6.1';
// Bump both together, after the Device panel is tried on the new release:
// the addon reaches private CoreSimulator frameworks that each Xcode moves.
const serveSimVersion = '0.1.48';
const serveSimSha256 = '07ce5aa9cc21081080bde29238ed257cf0ab9977608d90774ed4b31137fb6e58';

if (!/^v\d+\.\d+\.\d+$/.test(aiMemoryVersion)) {
  throw new Error(`invalid AI_MEMORY_VERSION: ${aiMemoryVersion}`);
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { stdio: 'inherit', cwd: repoRoot, ...options });
  if (result.error) {
    console.error(`[build-sidecars] failed to start ${command}:`, result.error);
    process.exit(1);
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

function detectHostTriple() {
  const result = spawnSync('rustc', ['-vV'], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'] });
  const hostLine = result.status === 0 ? result.stdout.split('\n').find((line) => line.startsWith('host: ')) : null;
  if (!hostLine) {
    throw new Error('rust host triple not found');
  }
  return hostLine.slice('host: '.length).trim();
}

function binaryName(baseName) {
  return process.platform === 'win32' ? `${baseName}.exe` : baseName;
}

function copyExecutable(source, target) {
  // Release binaries may be read-only (0555): keep repeated builds replaceable.
  if (existsSync(target)) {
    chmodSync(target, 0o755);
  }
  copyFileSync(source, target);
  chmodSync(target, 0o755);
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

  copyExecutable(executable, join(releaseDir, executableName));
  writeFileSync(join(releaseDir, 'ai-memory.version'), `${aiMemoryVersion}\n${actual}\n`);
  console.log(`[build-sidecars] staged ai-memory ${aiMemoryVersion} (${actual})`);
}

/** serve-sim's universal (arm64 + x86_64) addon, from its npm tarball. */
function stageServeSim() {
  const cacheDir = join(targetDir, 'serve-sim-cache', serveSimVersion);
  const archivePath = join(cacheDir, `serve-sim-${serveSimVersion}.tgz`);
  const extractDir = join(cacheDir, 'extract');
  mkdirSync(cacheDir, { recursive: true });
  if (!existsSync(archivePath)) {
    run('curl', ['-fsSL', `https://registry.npmjs.org/serve-sim/-/serve-sim-${serveSimVersion}.tgz`, '-o', archivePath]);
  }
  const actual = sha256(archivePath);
  if (actual !== serveSimSha256) {
    rmSync(archivePath, { force: true });
    throw new Error(`serve-sim checksum mismatch: expected ${serveSimSha256}, got ${actual}`);
  }
  rmSync(extractDir, { recursive: true, force: true });
  mkdirSync(extractDir, { recursive: true });
  run('tar', ['-xzf', archivePath, '-C', extractDir]);
  const addon = join(extractDir, 'package', 'dist', 'native', 'serve-sim-native.node');
  if (!existsSync(addon)) {
    throw new Error(`serve-sim ${serveSimVersion} has no dist/native/serve-sim-native.node`);
  }
  copyExecutable(addon, join(releaseDir, 'serve-sim-native.node'));
  console.log(`[build-sidecars] staged serve-sim ${serveSimVersion} (${actual})`);
}

const hostTriple = detectHostTriple();
mkdirSync(releaseDir, { recursive: true });

// Our SDK integration only; Claude Code is a user-installed prerequisite.
run('bun', ['build', '--compile', 'src/index.mjs', '--outfile', 'dist/dcc-claude-sidecar'], { cwd: sidecarDir });

run('cargo', ['build', '--release', '-p', 'dcc-server', '--bin', 'dcc', '--bin', 'dccd', '--bin', 'dccd-http']);
stageAiMemory(hostTriple);
if (process.platform === 'darwin') {
  stageServeSim();
}

console.log(`[build-sidecars] built dcc, dccd, dccd-http, ai-memory and the Claude helper for ${hostTriple}`);
