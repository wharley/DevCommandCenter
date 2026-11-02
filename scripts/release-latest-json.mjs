#!/usr/bin/env node
/**
 * The release's `latest.json`, in the Tauri updater's format: the feed both
 * a v0.2.0 Tauri desktop and the native app read (`data::updater`). Each
 * platform points at its archive on this release with the archive's
 * minisign signature (`<asset>.sig`, made with the same key).
 *
 *   node scripts/release-latest-json.mjs --version 0.3.0 --dir <assets> \
 *     --repo wharley/DevCommandCenter [--notes docs/releases/v0.3.0.md]
 *
 * Writes <assets>/latest.json; fails when an archive or its signature is missing.
 */
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

function argument(name) {
  const index = process.argv.indexOf(`--${name}`);
  return index >= 0 ? process.argv[index + 1] : undefined;
}

function fail(message) {
  console.error(`[release-latest-json] ${message}`);
  process.exit(1);
}

const version = argument('version');
const dir = argument('dir');
const repo = argument('repo');
const notesFile = argument('notes');
if (!version || !/^\d+\.\d+\.\d+$/.test(version) || !dir || !repo) {
  fail('usage: --version X.Y.Z --dir <assets> --repo owner/name [--notes file]');
}

// The asset each platform installs, named as the Tauri releases named them.
const ARCHIVES = {
  'darwin-aarch64': `Dev.Command.Center_${version}_aarch64.app.tar.gz`,
  'darwin-x86_64': `Dev.Command.Center_${version}_x64.app.tar.gz`,
  'linux-x86_64': `Dev.Command.Center_${version}_amd64.deb`,
};
// The installer-specific keys an updater tries first.
const ALIASES = {
  'darwin-aarch64-app': 'darwin-aarch64',
  'darwin-x86_64-app': 'darwin-x86_64',
  'linux-x86_64-deb': 'linux-x86_64',
};

const platforms = {};
for (const [platform, asset] of Object.entries(ARCHIVES)) {
  const signature = join(dir, `${asset}.sig`);
  if (!existsSync(join(dir, asset)) || !existsSync(signature)) {
    fail(`missing ${asset} or its .sig in ${dir}`);
  }
  platforms[platform] = {
    signature: readFileSync(signature, 'utf8').trim(),
    url: `https://github.com/${repo}/releases/download/v${version}/${asset}`,
  };
}
for (const [alias, platform] of Object.entries(ALIASES)) {
  platforms[alias] = platforms[platform];
}

const notes = notesFile && existsSync(notesFile) ? readFileSync(notesFile, 'utf8') : `Dev Command Center ${version}`;
writeFileSync(
  join(dir, 'latest.json'),
  `${JSON.stringify({ version, notes, pub_date: new Date().toISOString(), platforms }, null, 2)}\n`,
);
console.log(`[release-latest-json] wrote ${join(dir, 'latest.json')} (${Object.keys(platforms).length} platform keys)`);
