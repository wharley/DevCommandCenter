#!/usr/bin/env node
// A throwaway "desktop" for trying the app in the iOS Simulator or the Android
// emulator, all on this machine and never on the real DCC data:
//
//   - the relay with `wrangler dev` (local, nothing deployed);
//   - `dccd-http` (build it first: cargo build --bin dccd-http) on port 9877
//     with a temporary database, app-data dir and HOME, pointed at that relay;
//   - one project + workspace on a temporary git repo, and a second project
//     with no workspace yet (as the desktop has right after adding one);
//   - a pairing offer: prints the `dcc://pair` link and, with --open, opens it
//     in the booted simulator (`xcrun simctl openurl`); with --android it
//     forwards the relay port into the running emulator (`adb reverse`, the
//     emulator's 127.0.0.1 is itself) and opens the link there;
//   - approves the phone when it asks (the "Aprovar <aparelho>?" click of the
//     desktop), after printing its fingerprint; with --manual it waits for
//     `approve` or `deny` typed in the terminal instead;
//   - a fake Expo push service (`DCC_PUSH_API_URL`): every push the desktop
//     sends is printed and delivered to the booted simulator with
//     `xcrun simctl push`, in the APNs payload Expo builds (data under
//     `body`). The app only has a token without an EAS project when Metro
//     runs with EXPO_PUBLIC_DCC_DEV_PUSH_TOKEN=ExponentPushToken[simulator].
//
// Then type in the terminal:
//   agent [prompt]   start a Claude session in the workspace and send a prompt
//                    that needs a permission (uses your claude CLI login)
//   offer            a new pairing link
//   push <sessionId> deliver a sample push for that thread to the simulator
//   browser [url]    an agent asks to open the Browser (newest session)
//   computer         an agent asks to drive Figma/Safari on the Mac
//   edit             change a file in the task's repo (delivery screen)
//   approve | deny   answer the pending pairing (with --manual)
//   quit
//
//   node apps/mobile/scripts/desktop-fixture.mjs --open      (iOS Simulator)
//   node apps/mobile/scripts/desktop-fixture.mjs --android   (Android emulator)
import { execFileSync, spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, realpathSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { createServer as createHttpServer } from "node:http";
import { createServer } from "node:net";
import { homedir, tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import { randomBytes } from "node:crypto";

const root = join(dirname(fileURLToPath(import.meta.url)), "../../..");
const open = process.argv.includes("--open");
const manual = process.argv.includes("--manual");
const android = process.argv.includes("--android");
const adb = join(process.env.ANDROID_HOME ?? join(homedir(), "Library/Android/sdk"), "platform-tools/adb");
const httpPort = Number(process.env.DCC_FIXTURE_PORT ?? 9877);
const bundleId = "com.devcommandcenter.mobile";
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
const secondRepo = join(tmp, "site-demo");
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
// A local "origin" so pushes from the delivery screen go somewhere.
const origin = join(tmp, "origin.git");
execFileSync("git", ["init", "-q", "--bare", "-b", "main", origin]);
execFileSync("git", ["remote", "add", "origin", origin], { cwd: repo });
execFileSync("git", ["push", "-q", "-u", "origin", "main"], { cwd: repo });
mkdirSync(secondRepo);
writeFileSync(join(secondRepo, "index.html"), "<h1>site-demo</h1>\n");
execFileSync("git", ["init", "-q", "-b", "main"], { cwd: secondRepo });
execFileSync("git", ["add", "."], { cwd: secondRepo });
execFileSync("git", ["-c", "user.name=DCC", "-c", "user.email=dcc@example.invalid", "commit", "-qm", "init"], { cwd: secondRepo });

// What Expo hands APNs for a message of `/send`: the alert in `aps`, the
// message's `data` under `body` (expo-notifications reads it from there).
function deliverToSimulator(message) {
	const payload = {
		aps: { alert: { title: message.title, body: message.body }, sound: message.sound ?? "default" },
		body: message.data ?? {},
		experienceId: "@anonymous/dcc-mobile",
		scopeKey: "@anonymous/dcc-mobile",
	};
	const file = join(tmp, `push-${Date.now()}-${Math.random().toString(16).slice(2)}.apns`);
	writeFileSync(file, JSON.stringify(payload));
	try {
		execFileSync("xcrun", ["simctl", "push", "booted", bundleId, file]);
		console.log(`  → entregue ao simulador (${JSON.stringify(payload)})`);
	} catch (error) {
		console.error(`  simctl push falhou: ${error.message}`);
	}
}

const pushPort = await freePort();
const fakeExpo = createHttpServer((request, response) => {
	let raw = "";
	request.on("data", (chunk) => (raw += chunk));
	request.on("end", () => {
		const body = raw ? JSON.parse(raw) : null;
		let answer = { data: {} };
		if (request.url === "/send") {
			const messages = Array.isArray(body) ? body : [body];
			for (const message of messages) {
				console.log(`\n→ push para ${message.to}: "${message.title} · ${message.body}" ${JSON.stringify(message.data)}`);
				deliverToSimulator(message);
			}
			answer = { data: messages.map((_, n) => ({ status: "ok", id: `fixture-${Date.now()}-${n}` })) };
		}
		response.writeHead(200, { "content-type": "application/json" });
		response.end(JSON.stringify(answer));
	});
});
await new Promise((resolve) => fakeExpo.listen(pushPort, "127.0.0.1", resolve));

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
		DCC_RELAY_URL: `ws://127.0.0.1:${relayPort}`,
		DCC_PUSH_API_URL: `http://127.0.0.1:${pushPort}`,
		// Browser/computer-use requests come from the `browser`/`computer` commands.
		DCC_FIXTURE_AGENT_REQUESTS: "1",
	},
	stdio: ["ignore", "pipe", "pipe"],
});
let desktopLog = "";
desktop.stdout.on("data", (chunk) => (desktopLog += chunk));
desktop.stderr.on("data", (chunk) => (desktopLog += chunk));

const stop = () => {
	desktop.kill("SIGTERM");
	relay.kill("SIGTERM");
	fakeExpo.close();
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
INSERT OR IGNORE INTO dcc_repositories (id, project_id, name, display_name, icon, color, root_path, base_branch, created_at, updated_at)
VALUES ('fixture-repo-2', 'fixture-project-2', 'site-demo', 'site-demo', 'tucano', 'orange', '${secondRepo}', 'main', '${now}', '${now}');
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
	if (android) {
		try {
			execFileSync(adb, ["reverse", `tcp:${relayPort}`, `tcp:${relayPort}`]);
			// `am start` goes through the device shell: quote the link for it.
			execFileSync(adb, ["shell", `am start -a android.intent.action.VIEW -d '${created.url}'`]);
			console.log("(aberto no emulador)");
		} catch (error) {
			console.error(`adb failed: ${error.message}`);
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

/** A Browser or computer-use request from "the agent" of the newest session. */
async function agentRequest(kind, url) {
	const sessions = await api("GET", "/api/v1/sessions/search?limit=5");
	const session = sessions[0];
	if (!session) return console.log("crie uma sessão antes (agent)");
	const requestId = `${kind === "browser" ? "bo" : "cu"}-${randomBytes(6).toString("hex")}`;
	const common = {
		requestId,
		sessionId: session.sessionId,
		workspaceId: session.workspaceId ?? workspace,
		providerId: session.providerId ?? "claude_code",
		expiresAtMs: Date.now() + 45_000,
	};
	await api(
		"POST",
		"/api/v1/remote/fixture/agent-requests",
		kind === "browser"
			? { ...common, kind: "browserOpen", url: url || "https://docs.github.com/pt/actions", reason: "Ler a documentação do GitHub Actions para configurar o CI" }
			: {
					...common,
					kind: "computerControl",
					reason: "Exportar o frame do Figma que a tarefa pede",
					apps: [
						{ bundleId: "com.figma.Desktop", name: "Figma" },
						{ bundleId: "com.apple.Safari", name: "Safari" },
					],
					blocker: null,
				},
	);
	console.log(`${kind === "browser" ? "navegador" : "computer use"} pedido (${requestId}), 45 s para responder`);
}

/** A change in the task's repo, for the delivery screen. */
function edit() {
	writeFileSync(join(repo, "NOTAS.md"), `Mudança do fixture em ${new Date().toISOString()}\n`);
	execFileSync("git", ["switch", "-q", "-C", "dcc/fixture"], { cwd: repo });
	console.log("NOTAS.md alterado no repositório da tarefa (branch dcc/fixture)");
}

console.log(`relay ws://127.0.0.1:${relayPort} · dccd-http ${base} · push falso http://127.0.0.1:${pushPort} · dados em ${tmp}`);
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
			: command === "push" && rest[0]
				? Promise.resolve(
						deliverToSimulator({ title: "DCC", body: "loja-demo precisa de você", data: { sessionId: rest[0], kind: "permission" } }),
					)
			: command === "browser" || command === "computer"
				? agentRequest(command, rest[0])
			: command === "edit"
				? Promise.resolve(edit())
			: command === "agent"
				? agent(rest.join(" ") || "Rode `yarn test` no terminal e me diga o resultado em uma frase.")
				: command === "quit"
					? (stop(), process.exit(0))
					: Promise.resolve(
							command ? console.log("comandos: agent [prompt] | browser [url] | computer | edit | offer | push <sessionId> | approve | deny | quit") : undefined,
						);
	run.catch((error) => console.error(error.message));
});
