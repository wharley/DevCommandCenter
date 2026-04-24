#!/usr/bin/env node
/**
 * Verifica se a versão do Node.js é compatível com os requisitos do projeto.
 * Este script é executado como preinstall hook.
 */

import { readFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, "..");

// Cores ANSI
const colors = {
  reset: "\x1b[0m",
  bright: "\x1b[1m",
  red: "\x1b[31m",
  green: "\x1b[32m",
  yellow: "\x1b[33m",
  blue: "\x1b[34m",
};

function log(color, prefix, message) {
  console.log(`${color}${prefix}${colors.reset} ${message}`);
}

function logError(message) {
  log(colors.red, "✗", message);
}

function logWarn(message) {
  log(colors.yellow, "⚠", message);
}

function logInfo(message) {
  log(colors.blue, "ℹ", message);
}

function logSuccess(message) {
  log(colors.green, "✓", message);
}

// Lê a versão requerida do .nvmrc
let requiredVersion;
try {
  requiredVersion = readFileSync(join(ROOT, ".nvmrc"), "utf-8").trim();
} catch {
  logError(".nvmrc não encontrado");
  process.exit(1);
}

// Lê os engines do package.json
let engineRequirement;
try {
  const pkg = JSON.parse(readFileSync(join(ROOT, "package.json"), "utf-8"));
  engineRequirement = pkg.engines?.node;
} catch {
  logWarn("Não foi possível ler package.json");
}

// Parseia a versão atual do Node
const [currentMajor, currentMinor, currentPatch] = process.versions.node
  .split(".")
  .map((n) => parseInt(n, 10));

console.log("");
logInfo(`Versão do Node recomendada: v${requiredVersion}`);
if (engineRequirement) {
  logInfo(`Versão mínima requerida: ${engineRequirement}`);
}
logInfo(`Versão atual do Node: v${process.versions.node}`);

// Verifica se a versão é compatível
const isCompatible = currentMajor >= 18 && (currentMajor > 18 || currentMinor >= 18);

if (!isCompatible) {
  console.log("");
  logError(`Node.js ${engineRequirement || ">=18.18.0"} é obrigatório!`);
  console.log("");
  console.log(`${colors.bright}Para corrigir:${colors.reset}`);
  console.log("");
  console.log(`  1. ${colors.bright}Se você tem nvm instalado:${colors.reset}`);
  console.log(`     ${colors.blue}./setup.sh${colors.reset}`);
  console.log("");
  console.log(`  2. ${colors.bright}OU execute manualmente:${colors.reset}`);
  console.log(`     ${colors.blue}nvm use ${requiredVersion}${colors.reset}`);
  console.log(`     ${colors.blue}yarn install${colors.reset}`);
  console.log("");
  console.log(`  3. ${colors.bright}Se não tem nvm:${colors.reset}`);
  console.log(`     Instale em: ${colors.blue}https://github.com/nvm-sh/nvm${colors.reset}`);
  console.log("");
  process.exit(1);
}

// Aviso se não está usando a versão recomendada
if (currentMajor !== parseInt(requiredVersion, 10)) {
  console.log("");
  logWarn(`Recomendado usar Node v${requiredVersion} conforme .nvmrc`);
  logWarn(`Execute: nvm use ${requiredVersion}`);
  console.log("");
} else {
  console.log("");
  logSuccess("Versão do Node compatível!");
  console.log("");
}

process.exit(0);
