// Optional browser QA: point DCC_PLAYWRIGHT_MODULE at an installed Playwright module.
const { chromium } = await import(
	process.env.DCC_PLAYWRIGHT_MODULE || "playwright"
);
import http from "node:http";
import fs from "node:fs/promises";
import path from "node:path";
import assert from "node:assert/strict";
const dist = path.resolve(import.meta.dirname, "../dist");
const artifacts =
	process.env.DCC_SMOKE_ARTIFACTS ||
	path.join((await import("node:os")).tmpdir(), "dcc-mobile-smoke");
await fs.mkdir(artifacts, { recursive: true });
const now = new Date().toISOString();
const workspace = {
	id: "workspace",
	name: "Checkout mobile",
	rootPath: "/repo/dcc",
	baseBranch: "main",
	projectId: "project",
	state: "ready",
};
let events = [];
let created = 0;
let drop = true;
let latestInput;
const requests = new Map();
const streams = new Set();
function event(kind) {
	events.push({
		eventId: `e${events.length + 1}`,
		sessionId: "session",
		sequence: events.length + 1,
		occurredAt: now,
		kind,
	});
}
function notify() {
	for (const s of streams)
		s.write(
			"data: " +
				JSON.stringify({ sessionTurnCompleted: { session_id: "session" } }) +
				"\n\n",
		);
}
const json = (res, data, status = 200) => {
	res.writeHead(status, { "Content-Type": "application/json" });
	res.end(JSON.stringify(data));
};
const server = http.createServer(async (req, res) => {
	try {
		const url = new URL(req.url, "http://localhost");
		const p = url.pathname;
		if (p === "/auth/pair")
			return json(res, {
				ok: true,
				deviceId: "test-device",
				sessionToken: "test-only",
			});
		if (p === "/api/v1/events/stream") {
			res.writeHead(200, {
				"Content-Type": "text/event-stream",
				"Cache-Control": "no-cache",
			});
			res.write(": keep-alive\n\n");
			streams.add(res);
			req.on("close", () => streams.delete(res));
			return;
		}
		if (p === "/api/v1/mobile/catalog")
			return json(res, {
				providers: [
					{
						id: "codex",
						label: "Codex",
						enabled: true,
						health: "Healthy",
						models: [
							{
								id: "model",
								label: "Modelo configurado no host",
								recommended: true,
							},
						],
					},
					{
						id: "claude_code",
						label: "Claude Code",
						enabled: true,
						health: "Healthy",
						models: [],
					},
				],
				repositories: [
					{
						id: "repo",
						name: "DCC",
						rootPath: "/repo/dcc",
						baseBranch: "main",
						projectId: "project",
					},
				],
				workspaces: [workspace],
			});
		if (p === "/api/v1/mobile/tasks" && req.method === "POST") {
			let body = "";
			for await (const b of req) body += b;
			const input = JSON.parse(body);
			latestInput = input;
			if (!requests.has(input.requestId)) {
				created++;
				requests.set(input.requestId, {
					state: "started",
					sessionId: "session",
					workspaceId: "workspace",
				});
				event({ type: "turn_started", turnId: "turn", prompt: input.prompt });
				event({
					type: "turn_delta",
					turnId: "turn",
					content: "Implementação iniciada pelo celular.",
				});
				event({ type: "turn_completed", turnId: "turn" });
			}
			if (drop) {
				drop = false;
				res.writeHead(200, {
					"Content-Type": "application/json",
					"Content-Length": "9999",
				});
				res.write("{");
				setTimeout(() => res.destroy(), 100);
				return;
			}
			return json(res, requests.get(input.requestId));
		}
		if (p.startsWith("/api/v1/mobile/tasks/"))
			return json(res, requests.get(p.split("/").pop()));
		if (p === "/api/v1/status")
			return json(res, { running: true, cpuPercent: 3, memoryMb: 128 });
		if (p === "/api/v1/combs")
			return json(res, [
				{
					...workspace,
					branch: "main",
					projectName: "DCC",
					status: "ready",
					worktreePath: "/repo/dcc",
				},
			]);
		if (p === "/api/v1/sessions/search")
			return json(
				res,
				created
					? [
							{
								sessionId: "session",
								threadTitle: latestInput.title,
								providerId: "codex",
								workspaceName: "Checkout mobile",
								workspaceBranch: "main",
								workspaceId: "workspace",
								projectId: "project",
								updatedAt: now,
							},
						]
					: [],
			);
		if (p === "/api/v1/sessions/session/events") return json(res, events);
		if (p === "/api/v1/diffs/bundle")
			return json(res, [
				{
					worktreePath: "/repo/dcc",
					branch: "main",
					status: " M src/app.ts\n",
					stat: "1 file changed, 1 insertion(+), 1 deletion(-)",
					nameStatus: "M\tsrc/app.ts",
				},
			]);
		if (p === "/api/v1/mobile/workspaces/workspace/patch")
			return json(res, {
				patch:
					"diff --git a/src/app.ts b/src/app.ts\n@@ -1 +1 @@\n-oldValue\n+newValue",
				truncated: false,
			});
		if (p === "/api/v1/mobile/push")
			return json(res, { publicKey: "test", enabled: false });
		if (p === "/api/v1/sessions/session/respond-user-input") {
			let body = "";
			for await (const b of req) body += b;
			const answer = JSON.parse(body);
			assert.equal(answer.answers[0].answer, "Manter compatibilidade");
			event({
				type: "turn_user_input_resolved",
				requestId: "question-request",
				turnId: "turn",
				answers: answer.answers,
			});
			notify();
			return json(res, { ok: true });
		}
		if (p.startsWith("/api/"))
			return json(
				res,
				{ error: { message: `Mock endpoint missing: ${p}` } },
				404,
			);
		const file = path.join(dist, p.replace(/^\/m\/?/, ""));
		let data;
		let actual = file;
		try {
			data = await fs.readFile(file);
		} catch {
			actual = path.join(dist, "index.html");
			data = await fs.readFile(actual);
		}
		const type =
			{
				".html": "text/html",
				".js": "application/javascript",
				".css": "text/css",
				".webmanifest": "application/manifest+json",
				".png": "image/png",
			}[path.extname(actual)] || "application/octet-stream";
		res.writeHead(200, { "Content-Type": type });
		res.end(data);
	} catch (e) {
		console.error(e);
		res.writeHead(500);
		res.end("error");
	}
});
await new Promise((resolve) => server.listen(5199, "127.0.0.1", resolve));
const browser = await chromium.launch({
	headless: true,
	executablePath: process.env.DCC_CHROMIUM_EXECUTABLE,
});
const context = await browser.newContext({
	viewport: { width: 390, height: 844 },
	deviceScaleFactor: 2,
	isMobile: true,
	hasTouch: true,
});
const page = await context.newPage();
const errors = [];
page.on("pageerror", (e) => errors.push(e.message));
try {
	await page.goto("http://127.0.0.1:5199/m/pair#nonce=test");
	await page.getByPlaceholder("000000").fill("123456");
	await page.getByRole("button", { name: /Parear|Conectar/ }).click();
	await page.waitForURL("**/m/");
	await page.evaluate(() =>
		Promise.race([
			navigator.serviceWorker.ready.then(() => true),
			new Promise((_, reject) =>
				setTimeout(
					() => reject(new Error("Service worker did not activate")),
					15000,
				),
			),
		]),
	);
	await page
		.getByRole("link", { name: "Nova tarefa", exact: true })
		.last()
		.click();
	await page
		.getByPlaceholder("Descreva a mudança, o problema ou a ideia…")
		.fill("Melhorar a experiência de checkout no celular");
	await page
		.getByPlaceholder("Um nome para encontrar depois")
		.fill("Checkout pelo celular");
	await page.reload();
	await page
		.getByRole("heading", { name: "Nova tarefa", exact: true })
		.waitFor();
	assert.equal(
		await page
			.getByPlaceholder("Descreva a mudança, o problema ou a ideia…")
			.inputValue(),
		"Melhorar a experiência de checkout no celular",
	);
	await page.screenshot({
		path: path.join(artifacts, "new.png"),
		fullPage: true,
	});
	await page
		.getByRole("button", { name: "Iniciar tarefa", exact: true })
		.click();
	await page
		.getByRole("button", { name: "Verificar criação", exact: true })
		.waitFor();
	await page.reload();
	await page
		.getByRole("button", { name: "Verificar criação", exact: true })
		.click();
	await page.waitForURL("**/m/threads/session");
	await page
		.getByText("Implementação iniciada pelo celular.", { exact: true })
		.waitFor();
	assert.equal(created, 1);
	assert.equal(latestInput.repositoryId, "repo");
	assert.equal(latestInput.workspaceId, null);
	await page.screenshot({
		path: path.join(artifacts, "thread.png"),
		fullPage: true,
	});
	const input = page.locator("textarea").last();
	await input.fill("Rascunho para continuar depois");
	await input.press("Enter");
	await input.type("Segunda linha");
	await context.setOffline(true);
	await page.reload();
	await page
		.getByText("Implementação iniciada pelo celular.", { exact: true })
		.waitFor();
	assert.equal(
		await page.locator("textarea").last().inputValue(),
		"Rascunho para continuar depois\nSegunda linha",
	);
	await page.screenshot({
		path: path.join(artifacts, "offline.png"),
		fullPage: true,
	});
	event({
		type: "turn_started",
		turnId: "turn-2",
		prompt: "Segunda instrução",
	});
	event({
		type: "turn_delta",
		turnId: "turn-2",
		content: "Resposta durante desconexão.",
	});
	event({ type: "turn_completed", turnId: "turn-2" });
	await context.setOffline(false);
	await page
		.getByText("Resposta durante desconexão.", { exact: true })
		.waitFor({ timeout: 25000 });
	assert.equal(
		await page
			.getByText("Implementação iniciada pelo celular.", { exact: true })
			.count(),
		1,
	);
	event({
		type: "turn_user_input_requested",
		turnId: "turn-2",
		requestId: "question-request",
		questions: [
			{
				id: "q1",
				header: "Compatibilidade",
				question: "Qual abordagem devemos seguir?",
				options: [
					{
						label: "Manter compatibilidade",
						description: "Preserva clientes existentes",
					},
				],
			},
		],
	});
	notify();
	await page.getByRole("button", { name: "Manter compatibilidade" }).click();
	await page.getByRole("button", { name: "Enviar respostas" }).click();
	await page
		.getByRole("button", { name: "Enviar respostas" })
		.waitFor({ state: "hidden" });
	await page.goto("http://127.0.0.1:5199/m/diff/workspace");
	await page.getByRole("button", { name: /src.*app.ts/ }).click();
	await page.getByText("+newValue", { exact: true }).waitFor();
	await page.screenshot({
		path: path.join(artifacts, "diff.png"),
		fullPage: true,
	});
	await page.goto("http://127.0.0.1:5199/m/settings");
	await page
		.getByRole("heading", { name: "Notificações", exact: true })
		.waitFor();
	await page.screenshot({
		path: path.join(artifacts, "settings.png"),
		fullPage: true,
	});
	assert.equal(
		await page.evaluate(
			() => document.documentElement.scrollWidth > window.innerWidth,
		),
		false,
	);
	const cachedUrls = await page.evaluate(async () => {
		const urls = [];
		for (const name of await caches.keys())
			for (const req of await (await caches.open(name)).keys())
				urls.push(req.url);
		return urls;
	});
	assert.ok(cachedUrls.length > 5);
	assert.ok(cachedUrls.every((url) => new URL(url).pathname.startsWith("/m/")));
	assert.deepEqual(errors, []);
	console.log(
		JSON.stringify(
			{
				passed: true,
				checks: [
					"pairing",
					"mobile creation",
					"draft after reload",
					"lost creation response recovered once",
					"isolated workspace payload",
					"offline shell and history",
					"reconnection gap recovery",
					"agent questions",
					"file diff",
					"PWA settings",
					"no horizontal overflow",
					"no browser errors",
				],
				created,
				screenshots: artifacts,
			},
			null,
			2,
		),
	);
} catch (error) {
	console.error(await page.locator("body").innerText());
	await page.screenshot({
		path: path.join(artifacts, "failure.png"),
		fullPage: true,
	});
	throw error;
} finally {
	await browser.close();
	for (const stream of streams) stream.end();
	server.closeAllConnections();
	await new Promise((resolve) => server.close(resolve));
}
