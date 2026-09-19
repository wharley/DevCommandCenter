#!/usr/bin/env node
import { spawnSync } from "node:child_process";

const expectedVersion = "0.2.2";
const result = spawnSync("oj", ["--version"], {
	encoding: "utf8",
	stdio: ["ignore", "pipe", "pipe"],
});

if (result.error || result.status !== 0) {
	console.error(
		`[Dev Command Center] OJ ${expectedVersion} não encontrado. Instale com: cargo install oj --version ${expectedVersion} --locked`,
	);
	process.exit(1);
}

const actualVersion = result.stdout.trim().replace(/^oj\s+/, "");
if (actualVersion !== expectedVersion) {
	console.error(
		`[Dev Command Center] versão do OJ incompatível: esperada ${expectedVersion}, encontrada ${actualVersion || "desconhecida"}.`,
	);
	process.exit(1);
}
