/**
 * The app's channel layer against a real desktop: relay (wrangler dev) and
 * dccd-http from `scripts/desktop-fixture.mjs`, which auto-approves pairing.
 * Skipped unless a pairing link is given:
 *
 *   DCC_PAIR_LINK='dcc://pair#…' [DCC_E2E_AGENT=1] yarn workspace @dcc/mobile test test/live.test.ts
 *
 * With DCC_E2E_AGENT=1 it also starts a Claude session in the fixture's
 * workspace (your claude CLI login) and answers its permission request.
 */
import { runScan, triageGroups, type ChatMessage, type SessionSearchResult } from "@dcc/mobile-core";
import { generateKeyPair, parsePairingOffer, RemoteClient } from "@dcc/remote-protocol";
import { describe, expect, it } from "vitest";
import { RemoteBackend } from "../src/backend/remote";
import { ApiError, type ConnectionState } from "../src/backend/types";
import { ThreadSync, type ThreadSnapshot } from "../src/screens/threadSync";

const link = process.env.DCC_PAIR_LINK;

async function until<T>(label: string, read: () => T | null | undefined | false, timeoutMs = 60_000): Promise<T> {
	const deadline = Date.now() + timeoutMs;
	for (;;) {
		const value = read();
		if (value) return value;
		if (Date.now() > deadline) throw new Error(`timed out: ${label}`);
		await new Promise((r) => setTimeout(r, 200));
	}
}

describe.skipIf(!link)("live desktop", () => {
	it("pairs, reconnects, follows a thread and answers a permission", { timeout: 360_000 }, async () => {
		const offer = parsePairingOffer(link!);
		const device = { keyPair: generateKeyPair(), name: "vitest", platform: "node" };
		const client = await RemoteClient.pair(offer, device);
		expect(client.deviceId).toBeTruthy();

		const target = { relay: offer.relay, room: offer.room, desktopPublicKey: offer.desktopPublicKey };
		const backend = new RemoteBackend({ initial: client, connect: () => RemoteClient.connect(target, device) });
		const states: ConnectionState["kind"][] = [];
		backend.onState((state) => states.push(state.kind));

		const { combs } = await runScan((path) => backend.request("GET", path));
		expect(combs.map((c) => c.id)).toContain("fixture-workspace");

		// Drop the socket: the backend redials with Noise IK and comes back.
		client.close();
		await until("reconnected", () => backend.state().kind === "online" && states.includes("offline"), 20_000);
		await expect(backend.request("GET", "/api/v1/combs")).resolves.toBeTruthy();

		if (process.env.DCC_E2E_AGENT !== "1") {
			backend.dispose();
			return;
		}

		const started = await backend.request<{ session: { id: string } }>("POST", "/api/v1/sessions/start", {
			workspaceId: "fixture-workspace",
			projectId: "fixture-project",
			providerId: "claude_code",
			model: null,
			title: "live test",
		});
		const sessionId = started.session.id;
		let snapshot: ThreadSnapshot | null = null;
		const sync = new ThreadSync(backend, sessionId, (next) => (snapshot = next));
		sync.start();
		await backend.request("POST", `/api/v1/sessions/${sessionId}/turns`, {
			sessionId,
			prompt: "Rode `yarn test` no terminal e me diga o resultado em uma frase.",
		});

		const permission = await until(
			"permission request",
			() =>
				snapshot?.state.messages.find(
					(m): m is Extract<ChatMessage, { kind: "permission" }> => m.kind === "permission" && !m.resolved,
				),
			120_000,
		);
		const scan = await runScan((path) => backend.request("GET", path));
		const pending = triageGroups(scan.scan).needsYou.find((p) => p.requestId === permission.requestId);
		expect(pending?.thread.sessionId).toBe(sessionId);

		const before = snapshot!.state.cursor;
		const sessions = await backend.request<SessionSearchResult[]>("GET", "/api/v1/sessions/search?limit=10");
		expect(sessions.some((s) => s.sessionId === sessionId)).toBe(true);

		const respond = (requestId: string) =>
			backend.request("POST", `/api/v1/sessions/${sessionId}/respond-permission`, {
				sessionId,
				requestId,
				behavior: "allow",
			});
		await respond(permission.requestId);
		// The agent may ask again (network, another command): allow each one.
		const answered = new Set([permission.requestId]);
		const finished = await until(
			"turn finished",
			() => {
				const messages = snapshot?.state.messages ?? [];
				for (const m of messages) {
					if (m.kind === "permission" && !m.resolved && !answered.has(m.requestId)) {
						answered.add(m.requestId);
						void respond(m.requestId);
					}
				}
				const answer = [...messages].reverse().find((m) => m.kind === "assistant");
				return answer?.kind === "assistant" && answer.completed ? answer : null;
			},
			180_000,
		);
		expect(finished.text.length).toBeGreaterThan(0);
		expect(snapshot!.state.cursor).toBeGreaterThan(before);
		const again = await respond(permission.requestId).catch((e: unknown) => e);
		expect(again).toBeInstanceOf(ApiError);
		expect((again as ApiError).status).toBe(409);

		sync.stop();
		backend.dispose();
	});
});
