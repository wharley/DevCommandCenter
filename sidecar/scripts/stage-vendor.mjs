import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const pkgJson = require.resolve("@anthropic-ai/claude-code/package.json");
const packageDir = dirname(pkgJson);
const source = join(packageDir, "bin", "claude.exe");
const scriptDir = dirname(fileURLToPath(import.meta.url));
const sidecarDir = dirname(scriptDir);
const targetDir = join(sidecarDir, "dist", "vendor", "claude-code");
const targetName = process.platform === "win32" ? "claude.exe" : "claude";
const target = join(targetDir, targetName);

if (!existsSync(source)) {
	throw new Error(`Claude Code binary not found at ${source}`);
}

mkdirSync(targetDir, { recursive: true });
copyFileSync(source, target);
console.log(`[sidecar] staged Claude Code binary -> ${target}`);
