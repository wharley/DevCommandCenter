#!/usr/bin/env node
/**
 * Vite 6+ exige Node >= 18.18 (APIs em node:fs/promises).
 * Com Node 16 o `yarn vite` falha antes de subir o servidor → Tauri abre janela em branco.
 */
const major = parseInt(process.versions.node.split(".")[0], 10);
const minor = parseInt(process.versions.node.split(".")[1] ?? "0", 10);

if (Number.isNaN(major) || major < 18) {
  console.error(
    "\n[Dev Command Center] Node.js 18+ é obrigatório para o Vite 6 (README recomenda 22 LTS).\n" +
      `Versão atual: ${process.version}\n` +
      "Exemplo: nvm install 22 && nvm use 22\n",
  );
  process.exit(1);
}

if (major === 18 && minor < 18) {
  console.error(
    "\n[Dev Command Center] Para Vite 6 use Node >= 18.18.\n" +
      `Versão atual: ${process.version}\n`,
  );
  process.exit(1);
}

if (major < 20) {
  console.warn(
    `[Dev Command Center] Aviso: README recomenda Node 22 LTS. Atual: ${process.version}\n`,
  );
}

process.exit(0);
