import { conflictOutcome, outcomeText, parseAgentRequests } from "@dcc/mobile-core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DemoBackend } from "../src/backend/demo";
import type { StreamEvent } from "../src/backend/types";

const browser = {
	requestId: "bo-1",
	sessionId: "demo-s1",
	workspaceId: "demo-w1",
	providerId: "claude",
	reason: "Ver a doc do gateway",
	expiresAtMs: Date.now() + 45_000,
	kind: "browserOpen",
	url: "https://docs.example.com",
};
const computer = {
	...browser,
	requestId: "cu-1",
	kind: "computerControl",
	apps: [{ bundleId: "com.figma.Desktop", name: "Figma" }],
	blocker: null,
};

async function settle<T>(promise: Promise<T>): Promise<T> {
	await vi.advanceTimersByTimeAsync(500);
	return promise;
}

describe("agent requests (demo contract)", () => {
	beforeEach(() => vi.useFakeTimers());
	afterEach(() => vi.useRealTimers());

	it("arrive on the stream, are answered once and the late answer learns who won", async () => {
		const demo = new DemoBackend();
		const events: StreamEvent[] = [];
		demo.subscribe(() => "/api/v1/events/stream", { onEvent: (event) => events.push(event) });
		await vi.advanceTimersByTimeAsync(10);
		demo.openAgentRequest(browser);
		expect(events.at(-1)?.event).toBe("agent-requests");
		const listed = parseAgentRequests(await settle(demo.request("GET", "/api/v1/mobile/agent-requests")));
		expect(listed.map((r) => r.requestId)).toEqual(["bo-1"]);

		await settle(demo.request("POST", "/api/v1/mobile/agent-requests/bo-1/respond", { allow: true }));
		expect(parseAgentRequests(events.at(-1)?.data)).toEqual([]);
		expect(() => demo.answerAgentRequestOnDesktop("bo-1", false)).toThrow(/respondido/);
		demo.dispose();
	});

	it("show the desktop's answer when the phone is second", async () => {
		const demo = new DemoBackend();
		demo.openAgentRequest(computer);
		demo.answerAgentRequestOnDesktop("cu-1", false);
		const late = demo.request("POST", "/api/v1/mobile/agent-requests/cu-1/respond", { allow: true, allowedBundleIds: ["com.figma.Desktop"] });
		const caught = late.catch((error: unknown) => error as { status: number; body: unknown });
		const error = (await settle(caught)) as { status: number; body: unknown };
		expect(error.status).toBe(409);
		expect(outcomeText(conflictOutcome(error.body))).toBe("Esse pedido já foi negado no computador.");
		demo.dispose();
	});

	it("refuse computer use without an app", async () => {
		const demo = new DemoBackend();
		demo.openAgentRequest(computer);
		const caught = demo.request("POST", "/api/v1/mobile/agent-requests/cu-1/respond", { allow: true }).catch((e: unknown) => e as { status: number });
		expect(((await settle(caught)) as { status: number }).status).toBe(400);
		demo.dispose();
	});
});

describe("attention (demo contract)", () => {
	beforeEach(() => vi.useFakeTimers());
	afterEach(() => vi.useRealTimers());

	it("lists each task's reason in the desktop's shape and order", async () => {
		const { parseAttention, attentionLabel } = await import("@dcc/mobile-core");
		const demo = new DemoBackend();
		const entries = parseAttention(await settle(demo.request("GET", "/api/v1/mobile/attention")));
		const labels = entries.map((entry) => [entry.workspaceId, attentionLabel(entry)]);
		expect(labels).toContainEqual(["demo-w1", "Aguardando aprovação"]);
		expect(labels).toContainEqual(["demo-w4", "Checks falhando no PR #18"]);
		const times = entries.map((entry) => entry.returnedAt ?? "");
		expect([...times].sort().reverse()).toEqual(times);
		demo.dispose();
	});
});

describe("delivery (demo contract)", () => {
	beforeEach(() => vi.useFakeTimers());
	afterEach(() => vi.useRealTimers());

	it("commits with the shown message and token, refuses a stale view and opens a draft PR", async () => {
		const { deliveryActions } = await import("@dcc/mobile-core");
		const demo = new DemoBackend();
		const base = "/api/v1/mobile/workspaces/demo-w1/delivery";
		const summary = (await settle(demo.request("GET", base))) as import("@dcc/mobile-core").DeliverySummary;
		const forge = (await settle(demo.request("GET", `${base}/forge`))) as import("@dcc/mobile-core").DeliveryForge;
		expect(deliveryActions(summary, forge, false).actions[0]?.id).toBe("commit-and-push");
		expect(forge.requestTitle).toBe("Migrar o checkout para o novo gateway");
		await settle(demo.request("POST", `${base}/commit`, { changesToken: summary.changesToken, push: true }));
		const stale = demo.request("POST", `${base}/commit`, { changesToken: summary.changesToken }).catch((e: unknown) => e as { status: number });
		expect(((await settle(stale)) as { status: number }).status).toBe(409);
		await settle(demo.request("POST", `${base}/request`, { draft: true, title: null }));
		const after = (await settle(demo.request("GET", `${base}/forge`))) as import("@dcc/mobile-core").DeliveryForge;
		expect(after.pullRequest).toMatchObject({ number: 19, isDraft: true, title: "Migrar o checkout para o novo gateway" });
		demo.dispose();
	});
});
