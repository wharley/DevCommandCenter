#!/usr/bin/env node
/**
 * O runtime Claude Code empacotado exige Node 22; mantemos toda a toolchain no mesmo piso.
 * Com Node mais antigo o `yarn vite` falha ou emite avisos inconsistentes → Tauri pode abrir em branco.
 */
const [majS, minS, patS] = process.versions.node.split(".");
const major = parseInt(majS, 10);
const minor = parseInt(minS ?? "0", 10);
const patch = parseInt(patS ?? "0", 10);

function toolchainNodeOk() {
  if (Number.isNaN(major)) return false;
  if (major < 22) return false;
  if (major === 22) return minor > 12 || (minor === 12 && patch >= 0);
  return major >= 23;
}

if (!toolchainNodeOk()) {
  console.error(
    "\n[Dev Command Center] Node.js incompatível com a toolchain do projeto.\n" +
      "Requisito: >=22.12.0 (ver engines em package.json).\n" +
      `Versão atual: ${process.version}\n` +
      "Exemplo: nvm install 22 && nvm use 22\n",
  );
  process.exit(1);
}

process.exit(0);
