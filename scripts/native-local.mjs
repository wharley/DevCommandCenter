#!/usr/bin/env node
/**
 * Local build of the DCC (the native app, GPUI) installed as THE app:
 * "Dev Command Center" (com.devcommandcenter.app) in /Applications, on the
 * released app's data. It replaces the Tauri desktop app, which goes to the
 * Trash, and the side-by-side "Dev Command Center Native" of the test week.
 *
 *   yarn native:local                  build, snapshot, install, open
 *   node scripts/native-local.mjs build  build and sign only (no install)
 *   yarn native:local --skip-build     install the last build again
 *   yarn native:local --skip-sidecars  rebuild the app, reuse the built helpers
 *   yarn native:local --version 0.3.0  build as 0.3.0-local.g<commit>
 *   node scripts/native-local.mjs build --release 0.3.0
 *                                      the published build (CI): exactly 0.3.0,
 *                                      signed with a secure timestamp for notarization
 *   yarn native:rollback               put back the native build the last install replaced
 *   yarn native:rollback --with-db     ...and the database as it was at that moment
 *
 * The app hosts the backend (agents, terminals, the API) itself.
 */
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, readdirSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { basename, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(scriptDir, '..');
const nativeDir = join(repoRoot, 'apps', 'native');

const APP_NAME = 'Dev Command Center';
const APP_BUNDLE_NAME = `${APP_NAME}.app`;
const BUNDLE_ID = 'com.devcommandcenter.app';
const EXECUTABLE = 'dcc-native';
const INSTALL_PATH = join('/Applications', APP_BUNDLE_NAME);
const BUILT_APP_PATH = join(nativeDir, 'target', 'release', 'bundle', APP_BUNDLE_NAME);
// The Tauri desktop's executable: an install found with it is the old app.
const TAURI_EXECUTABLE = 'dev-command-center-tauri';
// The test week's side-by-side app, retired by the first install as THE app.
const SIDE_APP = { path: join('/Applications', 'Dev Command Center Native.app'), id: 'com.devcommandcenter.native' };
const DATA_DIR = join(homedir(), 'Library', 'Application Support', 'com.devcommandcenter.app');
const DATABASE = join(DATA_DIR, 'database.sqlite');
const SNAPSHOTS_DIR = join(homedir(), '.dcc', 'native-builds', 'snapshots');
const ROLLED_BACK_DIR = join(homedir(), '.dcc', 'native-builds', 'rolled-back');
const SNAPSHOTS_TO_KEEP = 3;
const MIN_NODE = [22, 12];

// What the bundle carries beside the app, as the desktop's does.
const SIDECARS = [
  join(repoRoot, 'target', 'release', 'dcc'),
  join(repoRoot, 'target', 'release', 'dccd'),
  join(repoRoot, 'target', 'release', 'dccd-http'),
  join(repoRoot, 'target', 'release', 'ai-memory'),
  join(repoRoot, 'sidecar', 'dist', 'dcc-claude-sidecar'),
  // The iOS Simulator's capture and input, loaded by the Claude helper; it
  // must carry our signature (library validation refuses another team's).
  join(repoRoot, 'target', 'release', 'serve-sim-native.node'),
];

function log(message) {
  console.log(`[native-local] ${message}`);
}

function fail(message) {
  console.error(`\n[native-local] ${message}\n`);
  process.exit(1);
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { stdio: 'inherit', cwd: repoRoot, ...options });
  if (result.error) {
    fail(`failed to start ${command}: ${result.error.message}`);
  }
  if (result.status !== 0) {
    fail(`${command} ${args.join(' ')} exited with ${result.status}`);
  }
}

function capture(command, args) {
  const result = spawnSync(command, args, { cwd: repoRoot, encoding: 'utf8' });
  return result.status === 0 ? result.stdout.trim() : '';
}

function sleep(ms) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

function timestamp() {
  const now = new Date();
  const pad = (value) => String(value).padStart(2, '0');
  return `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}-${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}`;
}

function bundleVersion(appPath) {
  return capture('/usr/libexec/PlistBuddy', ['-c', 'Print :CFBundleShortVersionString', join(appPath, 'Contents', 'Info.plist')]);
}

/** <next patch>-local.g<commit>, or <base>-local.g<commit> with --version. */
function nativeVersion(base) {
  const commit = capture('git', ['rev-parse', '--short', 'HEAD']) || 'unknown';
  const dirty = capture('git', ['status', '--porcelain', '--untracked-files=no']) ? '.dirty' : '';
  if (base) {
    if (!/^\d+\.\d+\.\d+$/.test(base)) {
      fail(`--version must look like 0.2.0, got ${base}`);
    }
    return `${base}-local.g${commit}${dirty}`;
  }
  // The released version lives in the app's own Cargo.toml.
  const manifest = readFileSync(join(nativeDir, 'Cargo.toml'), 'utf8');
  const version = /^version = "([^"]+)"/m.exec(manifest)?.[1] ?? '';
  const match = /^(\d+)\.(\d+)\.(\d+)$/.exec(version);
  if (!match) {
    fail(`unexpected version in apps/native/Cargo.toml: ${version}`);
  }
  return `${match[1]}.${match[2]}.${Number(match[3]) + 1}-local.g${commit}${dirty}`;
}

/** PATH with a Node that satisfies the sidecar build (>=22.12). */
function toolchainPath() {
  const [major, minor] = process.versions.node.split('.').map(Number);
  if (major > MIN_NODE[0] || (major === MIN_NODE[0] && minor >= MIN_NODE[1])) {
    return process.env.PATH;
  }
  const nvmVersionsDir = join(homedir(), '.nvm', 'versions', 'node');
  const candidates = existsSync(nvmVersionsDir)
    ? readdirSync(nvmVersionsDir)
        .map((name) => ({ name, parts: name.replace(/^v/, '').split('.').map(Number) }))
        .filter(({ parts: [maj, min] }) => maj > MIN_NODE[0] || (maj === MIN_NODE[0] && min >= MIN_NODE[1]))
        .sort((a, b) => b.parts[0] - a.parts[0] || b.parts[1] - a.parts[1] || b.parts[2] - a.parts[2])
    : [];
  if (candidates.length === 0) {
    fail(`Node ${process.version} is too old for the sidecar build and no Node >=${MIN_NODE.join('.')} was found in ~/.nvm.`);
  }
  log(`using Node ${candidates[0].name} from nvm for the sidecars`);
  return `${join(nvmVersionsDir, candidates[0].name, 'bin')}:${process.env.PATH}`;
}

/** The Developer ID identity, so macOS keeps granted permissions across builds
 * (the CI names it in APPLE_SIGNING_IDENTITY). */
function signingIdentity() {
  if (process.env.APPLE_SIGNING_IDENTITY) {
    return process.env.APPLE_SIGNING_IDENTITY;
  }
  const identities = capture('security', ['find-identity', '-v', '-p', 'codesigning']);
  const developerId = /"(Developer ID Application: [^"]+)"/.exec(identities);
  if (developerId) {
    return developerId[1];
  }
  log('no Developer ID identity in the keychain; signing ad hoc (macOS may ask for permissions again)');
  return '-';
}

function infoPlist(version) {
  return `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key><string>English</string>
  <key>CFBundleDisplayName</key><string>${APP_NAME}</string>
  <key>CFBundleExecutable</key><string>${EXECUTABLE}</string>
  <key>CFBundleIconFile</key><string>icon.icns</string>
  <key>CFBundleIdentifier</key><string>${BUNDLE_ID}</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>${APP_NAME}</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${version}</string>
  <key>CFBundleVersion</key><string>${version}</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
`;
}

function build(version, { skipSidecars, release = false }) {
  const env = { ...process.env, PATH: toolchainPath() };
  if (!skipSidecars) {
    // The same helpers the desktop ships: the CLI, the daemon, the headless
    // API, ai-memory and the Claude helper.
    log('building the helpers (dcc, dccd, dccd-http, ai-memory, Claude)');
    run('node', ['scripts/build-sidecars.mjs'], { env });
  }
  for (const sidecar of SIDECARS) {
    if (!existsSync(sidecar)) {
      fail(`missing helper ${sidecar}; run without --skip-sidecars.`);
    }
  }
  log(`building the native app ${version}`);
  // From its folder: apps/native pins its own toolchain (rust-toolchain.toml).
  run('cargo', ['build', '--release'], { env, cwd: nativeDir });

  rmSync(BUILT_APP_PATH, { recursive: true, force: true });
  const macos = join(BUILT_APP_PATH, 'Contents', 'MacOS');
  const resources = join(BUILT_APP_PATH, 'Contents', 'Resources');
  mkdirSync(macos, { recursive: true });
  mkdirSync(resources, { recursive: true });
  run('cp', [join(nativeDir, 'target', 'release', EXECUTABLE), join(macos, EXECUTABLE)]);
  for (const sidecar of SIDECARS) {
    run('cp', [sidecar, macos]);
  }
  run('cp', [join(nativeDir, 'assets', 'app-icon', 'icon.icns'), join(resources, 'icon.icns')]);
  run('cp', [join(repoRoot, 'sidecar', 'vendor', 'serve-sim', 'LICENSE'), join(resources, 'LICENSE-serve-sim')]);
  writeFileSync(join(BUILT_APP_PATH, 'Contents', 'Info.plist'), infoPlist(version));

  const identity = signingIdentity();
  log(`signing as "${identity}"`);
  // Helpers first, then the bundle (which seals them); hardened runtime like
  // the desktop's build.
  // Notarization wants Apple's secure timestamp; local builds skip the round trip.
  const timestampFlag = release ? '--timestamp' : '--timestamp=none';
  for (const name of readdirSync(macos).filter((name) => name !== EXECUTABLE)) {
    run('codesign', ['--force', '--options', 'runtime', timestampFlag, '--sign', identity, join(macos, name)]);
  }
  run('codesign', ['--force', '--options', 'runtime', timestampFlag, '--sign', identity, BUILT_APP_PATH]);
  run('codesign', ['--verify', '--strict', BUILT_APP_PATH]);
}

function pidsMatching(pattern) {
  return capture('pgrep', ['-f', pattern])
    .split('\n')
    .map((line) => Number(line.trim()))
    .filter((pid) => pid > 0 && pid !== process.pid);
}

function waitUntilGone(pattern, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (pidsMatching(pattern).length === 0) {
      return true;
    }
    sleep(250);
  }
  return pidsMatching(pattern).length === 0;
}

/** Quits the app at `path` (by its bundle id) and the helpers it started
 * from its own bundle, whichever build it is (native or the Tauri one). */
function stopApp(path, id, name) {
  const bundleProcesses = join(path, 'Contents', 'MacOS') + '/';
  const running = () => pidsMatching(bundleProcesses).length > 0;
  if (!running()) {
    return;
  }
  log(`quitting ${name}...`);
  spawnSync('osascript', ['-e', `tell application id "${id}" to quit`], { stdio: 'ignore' });
  if (!waitUntilGone(bundleProcesses, 30_000)) {
    const left = pidsMatching(bundleProcesses);
    log(`stopping ${left.length} helper process(es) left from ${name} (dccd, ai-memory, sidecars)`);
    spawnSync('kill', ['-TERM', ...left.map(String)], { stdio: 'ignore' });
    if (!waitUntilGone(bundleProcesses, 10_000)) {
      spawnSync('kill', ['-KILL', ...pidsMatching(bundleProcesses).map(String)], { stdio: 'ignore' });
      if (!waitUntilGone(bundleProcesses, 5_000)) {
        fail(`${name} did not quit. Close it and run again.`);
      }
    }
  }
}

function stopNativeApp() {
  stopApp(INSTALL_PATH, BUNDLE_ID, APP_NAME);
}

/** To the Trash (the person empties it), never deleted outright. */
function trash(path) {
  const target = join(homedir(), '.Trash', `${basename(path, '.app')} ${timestamp()}.app`);
  moveApp(path, target);
  log(`moved ${path} to the Trash`);
}

/** The Tauri desktop at the install path: replaced, not kept (the native app
 * is THE app now). */
function isTauriApp(path) {
  return existsSync(join(path, 'Contents', 'MacOS', TAURI_EXECUTABLE));
}

/** The test week's side-by-side native app: one DCC on the machine. */
function retireSideApp() {
  if (!existsSync(SIDE_APP.path)) {
    return;
  }
  stopApp(SIDE_APP.path, SIDE_APP.id, 'Dev Command Center Native');
  trash(SIDE_APP.path);
}

function listSnapshots() {
  if (!existsSync(SNAPSHOTS_DIR)) {
    return [];
  }
  return readdirSync(SNAPSHOTS_DIR)
    .filter((name) => existsSync(join(SNAPSHOTS_DIR, name, 'meta.json')))
    .sort();
}

function pruneSnapshots() {
  const snapshots = listSnapshots();
  for (const name of snapshots.slice(0, Math.max(0, snapshots.length - SNAPSHOTS_TO_KEEP))) {
    rmSync(join(SNAPSHOTS_DIR, name), { recursive: true, force: true });
  }
}

/** A consistent copy even while the desktop writes (SQLite's online backup). */
function backupDatabase(target) {
  if (!existsSync(DATABASE)) {
    return false;
  }
  mkdirSync(dirname(target), { recursive: true });
  run('sqlite3', [DATABASE, `.backup '${target.replaceAll("'", "''")}'`]);
  return true;
}

function moveApp(source, target) {
  mkdirSync(dirname(target), { recursive: true });
  try {
    renameSync(source, target);
  } catch {
    run('ditto', [source, target]);
    rmSync(source, { recursive: true, force: true });
  }
}

function install({ skipBuild, skipSidecars, baseVersion }) {
  if (process.platform !== 'darwin') {
    fail('native:local installs into /Applications and only runs on macOS.');
  }
  const version = nativeVersion(baseVersion);
  if (!skipBuild) {
    build(version, { skipSidecars });
  }
  if (!existsSync(BUILT_APP_PATH)) {
    fail(`no build at ${BUILT_APP_PATH}; run without --skip-build.`);
  }
  const builtVersion = bundleVersion(BUILT_APP_PATH);

  stopNativeApp();
  retireSideApp();

  const snapshotDir = join(SNAPSHOTS_DIR, timestamp());
  const tauri = existsSync(INSTALL_PATH) && isTauriApp(INSTALL_PATH);
  const replacedVersion = existsSync(INSTALL_PATH) ? bundleVersion(INSTALL_PATH) : null;
  log(`backing up the database${replacedVersion && !tauri ? ` and ${replacedVersion}` : ''} into ${snapshotDir}`);
  const backedUp = backupDatabase(join(snapshotDir, 'db', 'database.sqlite'));
  if (tauri) {
    log(`replacing the Tauri desktop ${replacedVersion}`);
    trash(INSTALL_PATH);
  } else if (replacedVersion) {
    moveApp(INSTALL_PATH, join(snapshotDir, APP_BUNDLE_NAME));
  }
  mkdirSync(snapshotDir, { recursive: true });
  writeFileSync(
    join(snapshotDir, 'meta.json'),
    `${JSON.stringify({ createdAt: new Date().toISOString(), replacedVersion, installedVersion: builtVersion, database: backedUp }, null, 2)}\n`,
  );
  pruneSnapshots();

  log(`installing ${builtVersion} into ${INSTALL_PATH}`);
  run('ditto', [BUILT_APP_PATH, INSTALL_PATH]);
  run('open', [INSTALL_PATH]);
  log(`done: ${replacedVersion ?? 'nothing'} -> ${builtVersion}. Undo with: yarn native:rollback`);
}

function rollback({ withDb }) {
  const latest = listSnapshots().at(-1);
  if (!latest) {
    fail(`no snapshot in ${SNAPSHOTS_DIR}.`);
  }
  const snapshotDir = join(SNAPSHOTS_DIR, latest);
  const meta = JSON.parse(readFileSync(join(snapshotDir, 'meta.json'), 'utf8'));
  const currentVersion = existsSync(INSTALL_PATH) ? bundleVersion(INSTALL_PATH) : null;

  stopNativeApp();

  // Keep the current native app and database once, in case the rollback
  // itself was the mistake.
  rmSync(ROLLED_BACK_DIR, { recursive: true, force: true });
  backupDatabase(join(ROLLED_BACK_DIR, 'db', 'database.sqlite'));
  if (currentVersion) {
    moveApp(INSTALL_PATH, join(ROLLED_BACK_DIR, APP_BUNDLE_NAME));
  }

  if (withDb) {
    const saved = join(snapshotDir, 'db', 'database.sqlite');
    if (!existsSync(saved)) {
      fail(`the snapshot has no database: ${saved}`);
    }
    log(`restoring the database as of ${meta.createdAt}`);
    for (const suffix of ['', '-wal', '-shm']) {
      rmSync(`${DATABASE}${suffix}`, { force: true });
    }
    run('cp', [saved, DATABASE]);
  }

  if (existsSync(join(snapshotDir, APP_BUNDLE_NAME))) {
    log(`restoring ${meta.replacedVersion}`);
    moveApp(join(snapshotDir, APP_BUNDLE_NAME), INSTALL_PATH);
    run('open', [INSTALL_PATH]);
  } else {
    log('no native build was installed before that one; it is removed.');
  }
  rmSync(snapshotDir, { recursive: true, force: true });
  log(`done: ${currentVersion ?? 'nothing'} -> ${meta.replacedVersion ?? 'nothing'}. The replaced app and database are in ${ROLLED_BACK_DIR}.`);
}

const [command, ...flags] = process.argv.slice(2);
// An unknown word must not fall through to installing: `yarn native:local
// build` is `install build` and would quit the running app.
const KNOWN_FLAGS = {
  install: ['--skip-build', '--skip-sidecars', '--version'],
  build: ['--skip-sidecars', '--version', '--release'],
  rollback: ['--with-db'],
};
const unknown = flags.filter(
  (flag, index) =>
    !['--version', '--release'].includes(flags[index - 1]) && !(KNOWN_FLAGS[command] ?? []).includes(flag),
);
if (unknown.length > 0) {
  fail(`unknown argument(s) for ${command}: ${unknown.join(' ')}. To only build, run: node scripts/native-local.mjs build`);
}
if (command === 'install') {
  const versionFlag = flags.indexOf('--version');
  install({
    skipBuild: flags.includes('--skip-build'),
    skipSidecars: flags.includes('--skip-sidecars'),
    baseVersion: versionFlag >= 0 ? flags[versionFlag + 1] : undefined,
  });
} else if (command === 'build') {
  const releaseFlag = flags.indexOf('--release');
  const versionFlag = flags.indexOf('--version');
  const release = releaseFlag >= 0 ? flags[releaseFlag + 1] : undefined;
  if (release !== undefined && !/^\d+\.\d+\.\d+$/.test(release)) {
    fail(`--release must look like 0.3.0, got ${release}`);
  }
  const version = release ?? nativeVersion(versionFlag >= 0 ? flags[versionFlag + 1] : undefined);
  build(version, { skipSidecars: flags.includes('--skip-sidecars'), release: release !== undefined });
  log(`built ${version} at ${BUILT_APP_PATH}`);
} else if (command === 'rollback') {
  rollback({ withDb: flags.includes('--with-db') });
} else {
  fail('usage: node scripts/native-local.mjs install [--skip-build] [--skip-sidecars] [--version X.Y.Z] | build [--skip-sidecars] [--version X.Y.Z | --release X.Y.Z] | rollback [--with-db]');
}
