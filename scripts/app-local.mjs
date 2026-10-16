#!/usr/bin/env node
/**
 * Local production build of DCC, installed over /Applications so it can be
 * used for real work (same data as the released app) before a release is cut.
 *
 *   yarn app:local                 build, snapshot, install, relaunch
 *   yarn app:local --skip-build    install the last build again
 *   yarn app:rollback              put back the app the last install replaced
 *   yarn app:rollback --with-db    ...and the database as it was at that moment
 *
 * The build is versioned <next patch>-local.g<commit> (for example
 * 0.1.99-local.g677c33b): newer than the published release, so the updater
 * never offers to downgrade it, and older than the next real release, so the
 * updater replaces it with that release on its own.
 */
import { spawnSync } from 'node:child_process';
import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  renameSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(scriptDir, '..');
const tauriConfigPath = join(repoRoot, 'src-tauri', 'tauri.conf.json');

const APP_BUNDLE_NAME = 'Dev Command Center.app';
const BUNDLE_ID = 'com.devcommandcenter.app';
const INSTALL_PATH = join('/Applications', APP_BUNDLE_NAME);
// The Cargo workspace root owns target/, not src-tauri/.
const BUILT_APP_PATH = join(repoRoot, 'target', 'release', 'bundle', 'macos', APP_BUNDLE_NAME);
const DATA_DIR = join(homedir(), 'Library', 'Application Support', BUNDLE_ID);
const DATABASE_FILES = ['database.sqlite', 'database.sqlite-wal', 'database.sqlite-shm'];
// One snapshot per install: the app it replaced plus the database at that moment.
const SNAPSHOTS_DIR = join(homedir(), '.dcc', 'local-builds', 'snapshots');
const ROLLED_BACK_DIR = join(homedir(), '.dcc', 'local-builds', 'rolled-back');
const SNAPSHOTS_TO_KEEP = 3;
const MIN_NODE = [22, 12];

function log(message) {
  console.log(`[app-local] ${message}`);
}

function fail(message) {
  console.error(`\n[app-local] ${message}\n`);
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

/** Local time, e.g. 20261016-191926, so snapshot names match the clock. */
function timestamp() {
  const now = new Date();
  const pad = (value) => String(value).padStart(2, '0');
  return `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}-${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}`;
}

function bundleVersion(appPath) {
  return capture('/usr/libexec/PlistBuddy', ['-c', 'Print :CFBundleShortVersionString', join(appPath, 'Contents', 'Info.plist')]);
}

function localVersion() {
  const { version } = JSON.parse(readFileSync(tauriConfigPath, 'utf8'));
  const match = /^(\d+)\.(\d+)\.(\d+)$/.exec(version);
  if (!match) {
    fail(`unexpected version in tauri.conf.json: ${version}`);
  }
  const commit = capture('git', ['rev-parse', '--short', 'HEAD']) || 'unknown';
  const dirty = capture('git', ['status', '--porcelain', '--untracked-files=no']) ? '.dirty' : '';
  // "g" keeps the identifier alphanumeric: an all-digit hash with a leading
  // zero is not valid semver.
  return `${match[1]}.${match[2]}.${Number(match[3]) + 1}-local.g${commit}${dirty}`;
}

/** PATH with a Node that satisfies the build toolchain (>=22.12). */
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
    fail(`Node ${process.version} is too old for the build and no Node >=${MIN_NODE.join('.')} was found in ~/.nvm.`);
  }
  log(`using Node ${candidates[0].name} from nvm for the build`);
  return `${join(nvmVersionsDir, candidates[0].name, 'bin')}:${process.env.PATH}`;
}

/** The Developer ID identity, so macOS keeps granted permissions across builds. */
function signingIdentity() {
  const identities = capture('security', ['find-identity', '-v', '-p', 'codesigning']);
  const developerId = /"(Developer ID Application: [^"]+)"/.exec(identities);
  if (developerId) {
    return developerId[1];
  }
  log('no Developer ID identity in the keychain; signing ad hoc (macOS may ask for permissions again)');
  return '-';
}

function build(version) {
  const identity = signingIdentity();
  const configPath = join(tmpdir(), `dcc-app-local-${process.pid}.json`);
  // The updater archive needs the release signing key, which only CI has.
  writeFileSync(configPath, JSON.stringify({ version, bundle: { createUpdaterArtifacts: false } }));

  const env = { ...process.env, PATH: toolchainPath(), APPLE_SIGNING_IDENTITY: identity };
  // Without these Tauri does not try to notarize; a build made on this Mac
  // carries no quarantine flag, so Gatekeeper does not need a ticket.
  for (const name of ['APPLE_ID', 'APPLE_PASSWORD', 'APPLE_TEAM_ID', 'APPLE_API_KEY', 'APPLE_API_ISSUER', 'APPLE_API_KEY_PATH', 'APPLE_CERTIFICATE', 'APPLE_CERTIFICATE_PASSWORD']) {
    delete env[name];
  }

  log(`building ${version} (signed as "${identity}")`);
  try {
    run('yarn', ['tauri', 'build', '--bundles', 'app', '--config', configPath], { env });
  } finally {
    rmSync(configPath, { force: true });
  }
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

/**
 * Quit DCC and stop every helper running from its bundle. dccd and ai-memory
 * outlive the app window; left running, the new app would keep talking to the
 * old daemon binary.
 */
function stopInstalledApp() {
  const bundleProcesses = join(INSTALL_PATH, 'Contents', 'MacOS') + '/';
  const mainProcess = join(INSTALL_PATH, 'Contents', 'MacOS', 'dev-command-center-tauri');

  if (pidsMatching(mainProcess).length > 0) {
    log('quitting Dev Command Center...');
    spawnSync('osascript', ['-e', `tell application id "${BUNDLE_ID}" to quit`], { stdio: 'ignore' });
    if (!waitUntilGone(mainProcess, 30_000)) {
      fail('Dev Command Center did not quit within 30s. Close it (answer any open dialog) and run again.');
    }
  }

  const helpers = pidsMatching(bundleProcesses);
  if (helpers.length > 0) {
    log(`stopping ${helpers.length} helper process(es) from the installed bundle (dccd, ai-memory, sidecars)`);
    spawnSync('kill', ['-TERM', ...helpers.map(String)], { stdio: 'ignore' });
    if (!waitUntilGone(bundleProcesses, 10_000)) {
      spawnSync('kill', ['-KILL', ...pidsMatching(bundleProcesses).map(String)], { stdio: 'ignore' });
      waitUntilGone(bundleProcesses, 5_000);
    }
  }
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

/** APFS clones: instant and they share blocks until either copy changes. */
function cloneDatabase(targetDir) {
  mkdirSync(targetDir, { recursive: true });
  for (const file of DATABASE_FILES) {
    const source = join(DATA_DIR, file);
    if (existsSync(source)) {
      run('cp', ['-c', source, join(targetDir, file)]);
    }
  }
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

function install({ skipBuild }) {
  if (process.platform !== 'darwin') {
    fail('app:local installs into /Applications and only runs on macOS.');
  }

  const version = localVersion();
  if (!skipBuild) {
    build(version);
  }
  if (!existsSync(BUILT_APP_PATH)) {
    fail(`no build at ${BUILT_APP_PATH}; run without --skip-build.`);
  }
  const builtVersion = bundleVersion(BUILT_APP_PATH);

  stopInstalledApp();

  const snapshotDir = join(SNAPSHOTS_DIR, timestamp());
  const replacedVersion = existsSync(INSTALL_PATH) ? bundleVersion(INSTALL_PATH) : null;
  log(`snapshotting the database${replacedVersion ? ` and ${replacedVersion}` : ''} into ${snapshotDir}`);
  cloneDatabase(join(snapshotDir, 'db'));
  if (replacedVersion) {
    moveApp(INSTALL_PATH, join(snapshotDir, APP_BUNDLE_NAME));
  }
  writeFileSync(
    join(snapshotDir, 'meta.json'),
    `${JSON.stringify({ createdAt: new Date().toISOString(), replacedVersion, installedVersion: builtVersion }, null, 2)}\n`,
  );
  pruneSnapshots();

  log(`installing ${builtVersion} into ${INSTALL_PATH}`);
  run('ditto', [BUILT_APP_PATH, INSTALL_PATH]);
  run('open', [INSTALL_PATH]);

  log(`done: ${replacedVersion ?? 'nothing'} -> ${builtVersion}. Undo with: yarn app:rollback`);
}

function rollback({ withDb }) {
  const snapshots = listSnapshots().filter((name) => existsSync(join(SNAPSHOTS_DIR, name, APP_BUNDLE_NAME)));
  const latest = snapshots.at(-1);
  if (!latest) {
    fail(`no snapshot with an app to restore in ${SNAPSHOTS_DIR}.`);
  }
  const snapshotDir = join(SNAPSHOTS_DIR, latest);
  const meta = JSON.parse(readFileSync(join(snapshotDir, 'meta.json'), 'utf8'));
  const currentVersion = existsSync(INSTALL_PATH) ? bundleVersion(INSTALL_PATH) : null;

  stopInstalledApp();

  // Keep the current app and database once, in case the rollback itself was
  // the mistake.
  rmSync(ROLLED_BACK_DIR, { recursive: true, force: true });
  cloneDatabase(join(ROLLED_BACK_DIR, 'db'));
  if (currentVersion) {
    moveApp(INSTALL_PATH, join(ROLLED_BACK_DIR, APP_BUNDLE_NAME));
  }

  if (withDb) {
    log(`restoring the database as of ${meta.createdAt}`);
    for (const file of DATABASE_FILES) {
      rmSync(join(DATA_DIR, file), { force: true });
      const saved = join(snapshotDir, 'db', file);
      if (existsSync(saved)) {
        run('cp', ['-c', saved, join(DATA_DIR, file)]);
      }
    }
  }

  log(`restoring ${meta.replacedVersion}`);
  moveApp(join(snapshotDir, APP_BUNDLE_NAME), INSTALL_PATH);
  rmSync(snapshotDir, { recursive: true, force: true });
  run('open', [INSTALL_PATH]);

  log(`done: ${currentVersion ?? 'nothing'} -> ${meta.replacedVersion}. The replaced app and database are in ${ROLLED_BACK_DIR}.`);
}

const [command, ...flags] = process.argv.slice(2);
if (command === 'install') {
  install({ skipBuild: flags.includes('--skip-build') });
} else if (command === 'rollback') {
  rollback({ withDb: flags.includes('--with-db') });
} else {
  fail('usage: node scripts/app-local.mjs install [--skip-build] | rollback [--with-db]');
}
