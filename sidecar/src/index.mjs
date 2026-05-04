#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import readline from "node:readline";

import { query } from "@anthropic-ai/claude-agent-sdk";

const SIDECAR_VERSION = "0.1.0";

function emit(value) {
	process.stdout.write(`${JSON.stringify(value)}\n`);
}

function resolveClaudeBinPath() {
	const override = process.env.DCC_CLAUDE_CODE_BIN_PATH;
	if (override && override.trim().length > 0) {
		return override.trim();
	}

	const require = createRequire(import.meta.url);
	const pkgJson = require.resolve("@anthropic-ai/claude-code/package.json");
	return join(dirname(pkgJson), "bin", "claude.exe");
}

const CLAUDE_BIN_PATH = resolveClaudeBinPath();

function handleAuthStatus() {
	const result = spawnSync(CLAUDE_BIN_PATH, ["auth", "status"], {
		stdio: ["ignore", "pipe", "pipe"],
		env: process.env,
		encoding: "utf8",
	});

	if (result.stdout) {
		process.stdout.write(result.stdout);
	}
	if (result.stderr) {
		process.stderr.write(result.stderr);
	}
	process.exit(result.status ?? 1);
}

function updateResumeSessionId(message, state) {
	if (
		message &&
		message.type === "system" &&
		message.subtype === "init" &&
		typeof message.session_id === "string" &&
		message.session_id.length > 0
	) {
		state.resumeSessionId = message.session_id;
	}
}

async function runTurn(prompt, state) {
	const q = query({
		prompt,
		options: {
			cwd: process.cwd(),
			pathToClaudeCodeExecutable: CLAUDE_BIN_PATH,
			model: process.env.DCC_MODEL || undefined,
			...(state.resumeSessionId ? { resume: state.resumeSessionId } : {}),
			permissionMode: "bypassPermissions",
			allowDangerouslySkipPermissions: true,
			includePartialMessages: true,
			settingSources: ["user", "project", "local"],
		},
	});
	let sawTerminalResult = false;

	try {
		for await (const message of q) {
			updateResumeSessionId(message, state);
			if (message && message.type === "result") {
				sawTerminalResult = true;
			}
			emit(message);
		}
	} catch (error) {
		if (!sawTerminalResult) {
			emit({
				type: "result",
				is_error: true,
				result: error instanceof Error ? error.message : String(error),
				session_id: state.resumeSessionId ?? null,
			});
		}
	} finally {
		try {
			q.close();
		} catch {
			// Ignore close failures during shutdown.
		}
	}
}

async function main() {
	if (process.argv.includes("--version")) {
		process.stdout.write(`dcc-claude-sidecar ${SIDECAR_VERSION}\n`);
		return;
	}

	if (process.argv.includes("--auth-status")) {
		handleAuthStatus();
		return;
	}

	const state = {
		resumeSessionId: null,
		running: false,
	};

	const rl = readline.createInterface({
		input: process.stdin,
		crlfDelay: Infinity,
		terminal: false,
	});

	for await (const rawLine of rl) {
		const line = rawLine.trim();
		if (!line) {
			continue;
		}

		let payload;
		try {
			payload = JSON.parse(line);
		} catch {
			emit({
				type: "result",
				is_error: true,
				result: "invalid Claude sidecar input payload",
				session_id: state.resumeSessionId ?? null,
			});
			continue;
		}

		if (payload.type !== "input" || typeof payload.prompt !== "string") {
			emit({
				type: "result",
				is_error: true,
				result: "unsupported Claude sidecar message",
				session_id: state.resumeSessionId ?? null,
			});
			continue;
		}

		if (state.running) {
			emit({
				type: "result",
				is_error: true,
				result: "Claude sidecar already has an active turn",
				session_id: state.resumeSessionId ?? null,
			});
			continue;
		}

		state.running = true;
		try {
			await runTurn(payload.prompt, state);
		} finally {
			state.running = false;
		}
	}
}

main().catch((error) => {
	process.stderr.write(
		`[dcc-claude-sidecar] ${error instanceof Error ? error.stack ?? error.message : String(error)}\n`,
	);
	process.exit(1);
});
