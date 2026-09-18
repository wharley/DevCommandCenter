import { existsSync } from 'node:fs';
import { spawn, spawnSync } from 'node:child_process';
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
