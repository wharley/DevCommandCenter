#!/usr/bin/env node
/**
 * The Linux package of the release: the app and its helpers (dcc, dccd,
 * dccd-http, ai-memory, the Claude helper) in /usr/bin, a launcher and the
 * icons, as `dev-command-center` — the Tauri desktop's own package name, so
 * `dpkg -i` (the updater's way) replaces it in place.
 *
 *   node scripts/package-linux.mjs --release 0.3.0
 *     → target/release/bundle/deb/Dev Command Center_0.3.0_amd64.deb
 *
 * Linux x86_64 only; needs dpkg-deb and the build deps the release job installs.
 */
import { spawnSync } from 'node:child_process';
import { chmodSync, copyFileSync, mkdirSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), '..');
const nativeDir = join(repoRoot, 'apps', 'native');
const PACKAGE = 'dev-command-center';
const BINARY = 'dev-command-center';
const HELPERS = [
  join(repoRoot, 'target', 'release', 'dcc'),
  join(repoRoot, 'target', 'release', 'dccd'),
  join(repoRoot, 'target', 'release', 'dccd-http'),
  join(repoRoot, 'target', 'release', 'ai-memory'),
  join(repoRoot, 'sidecar', 'dist', 'dcc-claude-sidecar'),
];
const ICONS = [
  ['32x32', '32x32.png'],
  ['64x64', '64x64.png'],
  ['128x128', '128x128.png'],
  ['256x256', '128x128@2x.png'],
];

function fail(message) {
  console.error(`\n[package-linux] ${message}\n`);
  process.exit(1);
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { stdio: 'inherit', cwd: repoRoot, ...options });
  if (result.status !== 0) {
    fail(`${command} ${args.join(' ')} exited with ${result.status ?? result.error?.message}`);
  }
}

const releaseFlag = process.argv.indexOf('--release');
const version = releaseFlag >= 0 ? process.argv[releaseFlag + 1] : undefined;
if (!version || !/^\d+\.\d+\.\d+$/.test(version)) {
  fail('usage: node scripts/package-linux.mjs --release X.Y.Z');
}
if (process.platform !== 'linux' || process.arch !== 'x64') {
  fail('the Linux package is built on Linux x86_64.');
}

run('node', ['scripts/build-sidecars.mjs']);
run('cargo', ['build', '--release'], { cwd: nativeDir });

const outDir = join(repoRoot, 'target', 'release', 'bundle', 'deb');
const root = join(outDir, `${PACKAGE}_${version}`);
rmSync(root, { recursive: true, force: true });
const bin = join(root, 'usr', 'bin');
mkdirSync(bin, { recursive: true });
copyFileSync(join(nativeDir, 'target', 'release', 'dcc-native'), join(bin, 'dcc-native'));
chmodSync(join(bin, 'dcc-native'), 0o755);
symlinkSync('dcc-native', join(bin, BINARY));
for (const helper of HELPERS) {
  const target = join(bin, helper.split('/').at(-1));
  copyFileSync(helper, target);
  chmodSync(target, 0o755);
}
for (const [size, file] of ICONS) {
  const dir = join(root, 'usr', 'share', 'icons', 'hicolor', size, 'apps');
  mkdirSync(dir, { recursive: true });
  copyFileSync(join(nativeDir, 'assets', 'app-icon', file), join(dir, `${PACKAGE}.png`));
}
const applications = join(root, 'usr', 'share', 'applications');
mkdirSync(applications, { recursive: true });
writeFileSync(
  join(applications, `${PACKAGE}.desktop`),
  `[Desktop Entry]
Type=Application
Name=Dev Command Center
Comment=Workbench for software engineering with AI agents
Exec=${BINARY}
Icon=${PACKAGE}
Terminal=false
Categories=Development;
StartupWMClass=dev-command-center
`,
);
const control = join(root, 'DEBIAN');
mkdirSync(control, { recursive: true });
const installedKb = Number(
  spawnSync('du', ['-sk', join(root, 'usr')], { encoding: 'utf8' }).stdout.split(/\s+/)[0] || 0,
);
writeFileSync(
  join(control, 'control'),
  `Package: ${PACKAGE}
Version: ${version}
Architecture: amd64
Installed-Size: ${installedKb}
Maintainer: devcommandcenter
Priority: optional
Section: devel
Depends: libwebkit2gtk-4.1-0, libgtk-3-0, libvulkan1, libxkbcommon0, libxkbcommon-x11-0, libfontconfig1, libssl3
Description: Dev Command Center
 Workbench for software engineering with AI agents: worktrees, sessions,
 review and delivery, terminals and a browser, in one native app.
`,
);
const debPath = join(outDir, `Dev Command Center_${version}_amd64.deb`);
run('dpkg-deb', ['--root-owner-group', '--build', root, debPath]);
console.log(`[package-linux] built ${debPath}`);
