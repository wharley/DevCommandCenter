import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { spawn, spawnSync } from 'node:child_process';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = join(fileURLToPath(new URL('.', import.meta.url)), '..');
const binaryName = process.platform === 'win32' ? 'ai-memory.exe' : 'ai-memory';

function findInstalledAiMemory() {
  if (process.env.DCC_AI_MEMORY_BIN && existsSync(process.env.DCC_AI_MEMORY_BIN)) {
    return process.env.DCC_AI_MEMORY_BIN;
  }
  if (process.env.HOME) {
    const applicationBinary = join(process.env.HOME, 'Applications', 'ai-memory', binaryName);
    if (existsSync(applicationBinary)) return applicationBinary;
  }
  const lookup = process.platform === 'win32' ? 'where.exe' : 'which';
  const result = spawnSync(lookup, ['ai-memory'], { encoding: 'utf8' });
  if (result.status === 0 && result.stdout?.trim()) {
    const candidate = result.stdout.trim().split(/\r?\n/)[0];
    if (existsSync(candidate)) return candidate;
  }
  return null;
}

// Tailwind 4 needs postcss 8 at the repository root. A reshuffled install once
// left react-scripts' postcss 7 there (via file-extension-icon-js), and the
// dev server answered every stylesheet with "plugin is not a function".
function assertRootPostcss() {
  const manifest = join(repoRoot, 'node_modules', 'postcss', 'package.json');
  if (!existsSync(manifest)) return;
  const { version } = JSON.parse(readFileSync(manifest, 'utf8'));
  if (Number.parseInt(version, 10) >= 8) return;
  console.error(
    `[dev] node_modules/postcss é ${version}; o Tailwind precisa do 8. Rode \`yarn install --force\` e tente de novo.`,
  );
  process.exit(1);
}

// The dev server sends its prebundled dependencies as immutable for a year, so
// after an install changes them the dev window keeps stale copies and lazy
// imports fail ("Importing a module script failed"). Drop the dev window's
// HTTP cache whenever yarn.lock changes; storage and the database are untouched.
function clearDevWebviewCacheIfLockChanged() {
  if (process.platform !== 'darwin') return;
  const lockfile = join(repoRoot, 'yarn.lock');
  if (!existsSync(lockfile)) return;
  const digest = createHash('sha256').update(readFileSync(lockfile)).digest('hex');
  const stampDir = join(repoRoot, 'node_modules', '.cache');
  const stamp = join(stampDir, 'dcc-dev-webview-yarn-lock');
  if (existsSync(stamp) && readFileSync(stamp, 'utf8') === digest) return;
  const cache = join(homedir(), 'Library', 'Caches', 'dev-command-center-tauri', 'WebKit', 'NetworkCache');
  if (existsSync(cache)) {
    try {
      rmSync(cache, { recursive: true, force: true, maxRetries: 3, retryDelay: 200 });
      console.log('[dev] yarn.lock mudou: cache de rede da janela dev limpo.');
    } catch (error) {
      // Another dev window is still writing to it; try again next start.
      console.warn(`[dev] não deu para limpar o cache da janela dev (${error.code}); feche a outra janela dev.`);
      return;
    }
  }
  mkdirSync(stampDir, { recursive: true });
  writeFileSync(stamp, digest);
}

assertRootPostcss();
clearDevWebviewCacheIfLockChanged();

const env = { ...process.env };
if (!env.DCC_AI_MEMORY_URL && !env.DCC_AI_MEMORY_DISABLE) {
  const binary = findInstalledAiMemory();
  if (binary) {
    env.DCC_AI_MEMORY_AUTO_START ||= '1';
    env.DCC_AI_MEMORY_BIN ||= binary;
  } else {
    console.warn(
      '[dev] ai-memory não encontrado; o DCC iniciará sem memória. Instale-o em ~/Applications/ai-memory ou defina DCC_AI_MEMORY_BIN para habilitar o sidecar.',
    );
  }
}

const command = process.platform === 'win32' ? 'tauri.cmd' : 'tauri';
const child = spawn(command, ['dev', '--config', 'src-tauri/tauri.dev.conf.json'], {
  cwd: repoRoot,
  env,
  stdio: 'inherit',
  shell: process.platform === 'win32',
});

child.on('error', (error) => {
  console.error(`[dev] failed to start Tauri: ${error.message}`);
  process.exitCode = 1;
});

child.on('exit', (code, signal) => {
  process.exitCode = code ?? (signal ? 1 : 0);
});
