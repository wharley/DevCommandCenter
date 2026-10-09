#!/usr/bin/env node
// A throwaway "desktop" for trying the app in the iOS Simulator, all on this
// machine and never on the real DCC data:
//
//   - the relay with `wrangler dev` (local, nothing deployed);
//   - `dccd-http` (build it first: cargo build --bin dccd-http) on port 9877
//     with a temporary database, app-data dir and HOME, pointed at that relay;
//   - one project + workspace on a temporary git repo;
//   - a pairing offer: prints the `dcc://pair` link and, with --open, opens it
//     in the booted simulator (`xcrun simctl openurl`);
//   - approves the phone when it asks (the "Aprovar <aparelho>?" click of the
//     desktop), after printing its fingerprint; with --manual it waits for
//     `approve` or `deny` typed in the terminal instead.
//
// Then type in the terminal:
//   agent [prompt]   start a Claude session in the workspace and send a prompt
//                    that needs a permission (uses your claude CLI login)
//   offer            a new pairing link
//   approve | deny   answer the pending pairing (with --manual)
//   quit
//
//   node apps/mobile/scripts/desktop-fixture.mjs --open
import { execFileSync, spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, realpathSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { homedir, tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import { randomBytes } from "node:crypto";

const root = join(dirname(fileURLToPath(import.meta.url)), "../../..");
const open = process.argv.includes("--open");
const manual = process.argv.includes("--manual");
const httpPort = Number(process.env.DCC_FIXTURE_PORT ?? 9877);
const apiKey = randomBytes(24).toString("hex");

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

async function waitFor(url, label, anyStatus = false) {
	const deadline = Date.now() + 90_000;
	for (;;) {
		try {
			const response = await fetch(url);
			if (response.ok || anyStatus) return;
		} catch {
			// not up yet
		}
		if (Date.now() > deadline) throw new Error(`${label} did not come up`);
		await new Promise((r) => setTimeout(r, 300));
	}
}

const binary = join(root, "target/debug/dccd-http");
if (!existsSync(binary)) {
	console.error("target/debug/dccd-http not found: run `cargo build --bin dccd-http` first.");
	process.exit(1);
}

// Physical path: the runtime registry refuses symlinked paths (/var → /private/var).
const tmp = realpathSync(mkdtempSync(join(tmpdir(), "dcc-mobile-fixture-")));
const home = join(tmp, "home");
const appData = join(tmp, "app");
const repo = join(tmp, "loja-demo");
mkdirSync(home, { recursive: true });
mkdirSync(appData, { recursive: true });
// The agent CLI reads its login from the real HOME (config plus the Keychain
// under ~/Library); what DCC writes (http-config.json, database, sidecars)
// stays in the temporary one. Removing the temp dir only unlinks these.
for (const name of [".claude", ".claude.json", ".gitconfig", "Library"]) {
	const source = join(homedir(), name);
	if (existsSync(source)) symlinkSync(source, join(home, name));
}
mkdirSync(repo);
writeFileSync(join(repo, "README.md"), "# loja-demo\n\nRepositório descartável do teste do app.\n");
writeFileSync(join(repo, "package.json"), '{ "name": "loja-demo", "scripts": { "test": "echo ok" } }\n');
execFileSync("git", ["init", "-q", "-b", "main"], { cwd: repo });
execFileSync("git", ["add", "."], { cwd: repo });
execFileSync("git", ["-c", "user.name=DCC", "-c", "user.email=dcc@example.invalid", "commit", "-qm", "init"], { cwd: repo });

const relayPort = await freePort();
const relay = spawn(
	process.execPath,
	[join(root, "node_modules/wrangler/bin/wrangler.js"), "dev", "--port", String(relayPort), "--ip", "127.0.0.1", "--persist-to", join(tmp, "relay")],
	{
		cwd: join(root, "apps/relay"),
		env: { ...process.env, WRANGLER_SEND_METRICS: "false", CLOUDFLARE_CF_FETCH_ENABLED: "false" },
		stdio: ["ignore", "ignore", "pipe"],
	},
);
const db = join(appData, "database.sqlite");
const desktop = spawn(binary, [], {
	env: {
		...process.env,
		HOME: home,
		DCC_APP_DATA_DIR: appData,
		DCC_HTTP_DB_PATH: db,
		DCC_HTTP_PORT: String(httpPort),
		DCC_HTTP_API_KEY: apiKey,
		DCC_HTTP_LAN: "0",
		DCC_RELAY_URL: `ws://127.0.0.1:${relayPort}`,
	},
	stdio: ["ignore", "pipe", "pipe"],
});
let desktopLog = "";
desktop.stdout.on("data", (chunk) => (desktopLog += chunk));
desktop.stderr.on("data", (chunk) => (desktopLog += chunk));

const stop = () => {
	desktop.kill("SIGTERM");
	relay.kill("SIGTERM");
	rmSync(tmp, { recursive: true, force: true });
};
for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"]) {
	process.on(signal, () => {
		stop();
		process.exit(130);
	});
}
process.on("uncaughtException", (error) => {
	console.error(error);
	stop();
	process.exit(1);
});
process.on("unhandledRejection", (error) => {
	console.error(error);
	stop();
	process.exit(1);
});
desktop.on("exit", (code) => {
	if (code !== null && code !== 0) {
		console.error(`dccd-http exited with ${code}\n${desktopLog}`);
		stop();
		process.exit(1);
	}
});

const base = `http://127.0.0.1:${httpPort}`;
await waitFor(`http://127.0.0.1:${relayPort}/health`, "relay");
// Answers 504 "degraded" without the separate dccd daemon; up is enough.
await waitFor(`${base}/`, "dccd-http", true);

async function api(method, path, body) {
	const response = await fetch(base + path, {
		method,
		headers: { "x-api-key": apiKey, ...(body ? { "content-type": "application/json" } : {}) },
		body: body ? JSON.stringify(body) : undefined,
	});
	const text = await response.text();
	if (!response.ok) throw new Error(`${method} ${path} → ${response.status} ${text}`);
	return text ? JSON.parse(text) : null;
}

// Project + workspace, as the desktop would have created them.
const now = new Date().toISOString();
const project = "fixture-project";
const workspace = "fixture-workspace";
// dcc_repositories comes from the desktop's workspace store, which dccd-http
// does not open; same columns as crates/dcc-infra/src/db.rs (REPOSITORY_TABLE_SQL + the logo migration).
const sql = `
CREATE TABLE IF NOT EXISTS dcc_repositories (
	id TEXT PRIMARY KEY NOT NULL, project_id TEXT NOT NULL, name TEXT NOT NULL,
	display_name TEXT NULL, icon TEXT NULL, color TEXT NULL, pinned_at TEXT NULL,
	root_path TEXT NOT NULL UNIQUE, base_branch TEXT NOT NULL, remote TEXT NULL,
	remote_url TEXT NULL, forge_provider TEXT NULL, forge_login TEXT NULL, logo TEXT NULL,
	created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
INSERT OR IGNORE INTO dcc_repositories (id, project_id, name, display_name, root_path, base_branch, created_at, updated_at)
VALUES ('fixture-repo', '${project}', 'loja-demo', 'loja-demo', '${repo}', 'main', '${now}', '${now}');
INSERT OR IGNORE INTO dcc_workspaces (id, project_id, name, root_path, base_branch, worktree_path, state, created_at, updated_at)
VALUES ('${workspace}', '${project}', 'main', '${repo}', 'main', '${repo}', 'ready', '${now}', '${now}');
`;
try {
	execFileSync("sqlite3", [db, sql]);
} catch (error) {
	console.error(`Could not seed the workspace (schema not ready?): ${error.message}`);
}

const seenApprovals = new Set();
const waiting = [];
setInterval(async () => {
	try {
		const { approvals } = await api("GET", "/api/v1/remote/pairing/approvals");
		for (const approval of approvals) {
			if (seenApprovals.has(approval.id)) continue;
			seenApprovals.add(approval.id);
			if (manual) {
				waiting.push(approval.id);
				console.log(`\n→ Aprovar ${approval.deviceName} (${approval.platform ?? "?"})? fingerprint ${approval.fingerprint} — digite approve ou deny.`);
				continue;
			}
			console.log(`\n→ Aprovar ${approval.deviceName} (${approval.platform ?? "?"})? fingerprint ${approval.fingerprint} — aprovando.`);
			await api("POST", `/api/v1/remote/pairing/approvals/${approval.id}`, { approve: true });
		}
	} catch (error) {
		console.error(`approvals: ${error.message}`);
	}
}, 1000);

async function offer() {
	const created = await api("POST", "/api/v1/remote/pairing/offers");
	console.log(`\nComputador: ${created.desktopFingerprint} (expira ${created.expiresAt})\n${created.url}\n`);
	if (open) {
		try {
			execFileSync("xcrun", ["simctl", "openurl", "booted", created.url]);
			console.log("(aberto no simulador)");
		} catch (error) {
			console.error(`simctl openurl failed: ${error.message}`);
		}
	}
}

async function agent(prompt) {
	const started = await api("POST", "/api/v1/sessions/start", {
		workspaceId: workspace,
		projectId: project,
		providerId: process.env.DCC_FIXTURE_PROVIDER ?? "claude_code",
		model: null,
		title: "Teste do app no simulador",
	});
	const sessionId = started.session?.id ?? started.sessionId ?? started.id;
	await api("POST", `/api/v1/sessions/${sessionId}/turns`, { sessionId, prompt });
	console.log(`sessão ${sessionId} iniciada`);
}

console.log(`relay ws://127.0.0.1:${relayPort} · dccd-http ${base} · dados em ${tmp}`);
await offer();
const rl = createInterface({ input: process.stdin });
rl.on("line", (line) => {
	const [command, ...rest] = line.trim().split(" ");
	const run =
		command === "approve" || command === "deny"
			? waiting.length
				? api("POST", `/api/v1/remote/pairing/approvals/${waiting.shift()}`, { approve: command === "approve" }).then(() =>
						console.log(command === "approve" ? "aprovado" : "recusado"),
					)
				: Promise.resolve(console.log("nenhum pedido de pareamento esperando"))
			: command === "offer"
			? offer()
			: command === "agent"
				? agent(rest.join(" ") || "Rode `yarn test` no terminal e me diga o resultado em uma frase.")
				: command === "quit"
					? (stop(), process.exit(0))
					: Promise.resolve(command ? console.log("comandos: agent [prompt] | offer | approve | deny | quit") : undefined);
	run.catch((error) => console.error(error.message));
});
