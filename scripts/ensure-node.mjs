#!/usr/bin/env node
/**
 * Vite 8 + @vitejs/plugin-react 6 exigem Node ^20.19.0 || >=22.12.0 (engines do plugin).
 * Com Node mais antigo o `yarn vite` falha ou emite avisos inconsistentes → Tauri pode abrir em branco.
 */
const [majS, minS, patS] = process.versions.node.split(".");
const major = parseInt(majS, 10);
const minor = parseInt(minS ?? "0", 10);
const patch = parseInt(patS ?? "0", 10);

function toolchainNodeOk() {
  if (Number.isNaN(major)) return false;
  if (major < 20) return false;
  if (major === 20) return minor > 19 || (minor === 19 && patch >= 0);
  if (major === 21) return false;
  if (major === 22) return minor > 12 || (minor === 12 && patch >= 0);
  return major >= 23;
}

if (!toolchainNodeOk()) {
  console.error(
    "\n[Dev Command Center] Node.js incompatível com Vite 8 / @vitejs/plugin-react.\n" +
      "Requisito: ^20.19.0 || >=22.12.0 (ver engines em package.json).\n" +
      `Versão atual: ${process.version}\n` +
      "Exemplo: nvm install 22 && nvm use 22\n",
  );
  process.exit(1);
}

if (major < 22) {
  console.warn(
    `[Dev Command Center] Aviso: README/.nvmrc recomendam Node 22 LTS. Atual: ${process.version}\n`,
  );
}

process.exit(0);
