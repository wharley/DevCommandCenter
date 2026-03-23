#!/usr/bin/env node
/**
 * Lista operações ainda não implementadas em `src-tauri/src/main.rs`
 * (mapped_not_implemented / ApiError::not_implemented).
 *
 * Uso: yarn audit:tauri-stubs
 */
import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const mainPath = path.join(__dirname, "..", "src-tauri", "src", "main.rs");
const text = fs.readFileSync(mainPath, "utf8");
const lines = text.split("\n");

const stubs = [];
for (let i = 0; i < lines.length; i++) {
  const line = lines[i];
  const m1 = line.match(/mapped_not_implemented\("([^"]+)"\)/);
  const m2 = line.match(/ApiError::not_implemented\("([^"]+)"\)/);
  if (m1) stubs.push({ line: i + 1, op: m1[1] });
  if (m2) stubs.push({ line: i + 1, op: m2[1] });
}

console.log("Tauri — comandos ainda stub (main.rs)\n");
console.log(`Total: ${stubs.length}\n`);
for (const { line, op } of stubs) {
  console.log(`${String(line).padStart(5)}  ${op}`);
}
