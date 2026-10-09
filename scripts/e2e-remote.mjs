#!/usr/bin/env node
// End-to-end test of remote access through the relay, all on this machine:
// starts the relay with `wrangler dev` (no Cloudflare account, nothing
// deployed), then runs `src-tauri/tests/remote_relay_e2e.rs`, which serves
// the API like the desktop app on a temporary database and drives a Node
// "phone" (packages/remote-protocol/test/e2e/phone.ts) through the relay.
//
//   node scripts/e2e-remote.mjs
import { spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const [major, minor] = process.versions.node.split(".").map(Number);
if (major < 22 || (major === 22 && minor < 18)) {
	console.error(`Node ${process.versions.node}: needs 22.18+ (type stripping and WebSocket).`);
	process.exit(1);
}

function freePort() {
	return new Promise((resolve, reject) => {
		const server = createServer();
		server.listen(0, "127.0.0.1", () => {
			const { port } = server.address();
			server.close(() => resolve(port));
		});
		server.on("error", reject);
	});
}

const port = await freePort();
const state = mkdtempSync(join(tmpdir(), "dcc-relay-e2e-"));
const relay = spawn(
	process.execPath,
	[
		join(root, "node_modules/wrangler/bin/wrangler.js"),
		"dev",
		"--port",
		String(port),
		"--ip",
		"127.0.0.1",
		"--var",
		"DCC_RELAY_DEBUG_TRAFFIC:1",
		"--persist-to",
		state,
	],
	{
		cwd: join(root, "apps/relay"),
		env: {
			...process.env,
			WRANGLER_SEND_METRICS: "false",
			CLOUDFLARE_CF_FETCH_ENABLED: "false",
		},
		stdio: ["ignore", "pipe", "pipe"],
	},
);
let relayLog = "";
relay.stdout.on("data", (chunk) => (relayLog += chunk));
relay.stderr.on("data", (chunk) => (relayLog += chunk));

const stop = () => {
	relay.kill("SIGTERM");
	rmSync(state, { recursive: true, force: true });
};
process.on("SIGINT", () => {
	stop();
	process.exit(130);
});

const deadline = Date.now() + 60_000;
for (;;) {
	try {
		const response = await fetch(`http://127.0.0.1:${port}/health`);
		if (response.ok) break;
	} catch {
		// not up yet
	}
	if (Date.now() > deadline || relay.exitCode !== null) {
		console.error(relayLog);
		console.error("the relay did not start");
		stop();
		process.exit(1);
	}
	await new Promise((resolve) => setTimeout(resolve, 250));
}
console.log(`relay on ws://127.0.0.1:${port}`);

const test = spawn(
	"cargo",
	[
		"test",
		"-p",
		"dev-command-center-tauri",
		"--test",
		"remote_relay_e2e",
		"--",
		"--nocapture",
	],
	{
		cwd: root,
		env: {
			...process.env,
			DCC_E2E_RELAY_URL: `ws://127.0.0.1:${port}`,
			DCC_E2E_NODE: process.execPath,
		},
		stdio: "inherit",
	},
);
const code = await new Promise((resolve) => test.on("exit", (exitCode) => resolve(exitCode ?? 1)));
if (code !== 0) {
	console.error("--- relay log ---");
	console.error(relayLog.split("\n").filter((line) => !line.includes("explorer")).join("\n"));
}
stop();
process.exit(code);
