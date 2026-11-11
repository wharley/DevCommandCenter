import { parseRoom, parseRoomSent, parseRoomSteps, liveLabel, liveSeconds, roomCardText, roomItems, roomTriggerLabel, roomVerdictLabel } from "@dcc/mobile-core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DemoBackend } from "../src/backend/demo";

/** A request to the demo, which answers after its own short delay. */
async function call(demo: DemoBackend, method: string, path: string, body?: unknown): Promise<unknown> {
	const answer = demo.request(method, path, body);
	await vi.advanceTimersByTimeAsync(120);
	return answer;
}

async function room(demo: DemoBackend, query = "") {
	const read = parseRoom(await call(demo, "GET", `/api/v1/mobile/room${query}`), null);
	if (!read || read.unchanged) throw new Error("no room");
	return read;
}

describe("demo rooms", () => {
	beforeEach(() => vi.useFakeTimers());
	afterEach(() => vi.useRealTimers());

	it("shows an agent at work live, queues what comes meanwhile and answers in turn", async () => {
		const demo = new DemoBackend();
		const sent = parseRoomSent(await call(demo, "POST", "/api/v1/mobile/room/messages", { text: "@Dev olha o cache" }));
		expect(sent?.queued).toBe(false);
		await vi.advanceTimersByTimeAsync(2_000);
		let read = await room(demo);
		const live = read.live.find((turn) => turn.agentId === "demo-dev");
		expect(live?.kind).toBe("conversation");
		expect(liveLabel(live!, liveSeconds(live!, Date.now()))).toBe("Buscando PAYMENT_FLAG");
		// Only the turns moved: the lines held are not sent again.
		const again = await room(demo, `?lines=${read.linesId}`);
		expect(again.linesUnchanged).toBe(true);
		const queued = parseRoomSent(await call(demo, "POST", "/api/v1/mobile/room/messages", { text: "@Dev e o deploy?" }));
		expect(queued?.queued).toBe(true);
		read = await room(demo);
		expect(read.queued.map((item) => item.text)).toEqual(["@Dev e o deploy?"]);
		await vi.advanceTimersByTimeAsync(5_000);
		read = await room(demo);
		expect(read.live.find((turn) => turn.agentId === "demo-dev")?.text.length).toBeGreaterThan(0);
		await vi.advanceTimersByTimeAsync(2_000);
		read = await room(demo);
		// The answer came, and the queued message went in its turn.
		expect(read.queued).toEqual([]);
		expect(read.lines.at(-1)?.text).toBe("@Dev e o deploy?");
		expect(read.live.some((turn) => turn.agentId === "demo-dev")).toBe(true);
		demo.dispose();
	});

	it("stops a turn, tries it again, passes the turn and lists an answer's steps", async () => {
		const demo = new DemoBackend();
		await call(demo, "POST", "/api/v1/mobile/room/messages", { text: "@Suporte o que chegou?" });
		let read = await room(demo);
		const turn = read.live.find((item) => item.agentId === "demo-sup")!;
		await call(demo, "POST", `/api/v1/sessions/${turn.sessionId}/abort`, { sessionId: turn.sessionId });
		read = await room(demo);
		const stopped = read.stopped.find((item) => item.agentId === "demo-sup")!;
		expect(stopped.retry?.prompt).toBe("@Suporte o que chegou?");
		await call(demo, "POST", "/api/v1/mobile/room/messages", {
			text: stopped.retry!.prompt,
			sessionId: stopped.retry!.sessionId,
			retryOf: stopped.retry!.turnId,
		});
		read = await room(demo);
		expect(read.stopped).toEqual([]);
		expect(read.live.some((item) => item.agentId === "demo-sup")).toBe(true);
		const suggests = read.lines.find((line) => line.handoff?.state === "pending")!;
		expect(suggests.handoff?.toName).toBe("Dev");
		await call(demo, "POST", "/api/v1/mobile/room/handoff", { key: suggests.key, action: "pass" });
		read = await room(demo);
		expect(read.lines.find((line) => line.key === suggests.key)?.handoff?.state).toBe("passed");
		expect(read.live.some((item) => item.agentId === "demo-dev")).toBe(true);
		const steps = parseRoomSteps(await call(demo, "GET", `/api/v1/mobile/room/steps?sessionId=${suggests.sessionId}&turn=${suggests.turnId}`));
		expect(steps?.steps.length).toBeGreaterThan(0);
		// Dev's task runs under loja-online, with the stop to try again there.
		const loja = await room(demo, "?room=loja");
		expect(loja.live.find((item) => item.kind === "task")?.task?.title).toBe("checkout-v2");
		expect(loja.stopped[0]?.retry).not.toBeNull();
		demo.dispose();
	});
	it("shows the desktop's cards, answers a proposal and keeps a reaction", async () => {
		const demo = new DemoBackend();
		let geral = await room(demo);
		expect(geral.cards.map((card) => card.kind)).toEqual(["prReview", "checkIn", "agentProposal"]);
		const review = geral.cards.find((card) => card.kind === "prReview")!;
		expect(roomVerdictLabel(review.verdict!)).toBe("Pode aprovar");
		const loja = await room(demo, "?room=loja");
		const done = loja.cards.find((card) => card.kind === "taskResult")!;
		expect(roomCardText(done, "Dev")).toBe("Dev terminou em corrigir-frete");
		expect(roomTriggerLabel(done.trigger)).toBe("Gatilho · CI falhou");
		// In time order among the lines, the proposal after them all.
		const items = roomItems(geral.lines, new Date(), geral.cards);
		const last = items.at(-1);
		expect(last?.type === "card" ? last.card.kind : null).toBe("agentProposal");
		const proposal = geral.cards.find((card) => card.kind === "agentProposal")!;
		await call(demo, "POST", "/api/v1/mobile/room/proposals", { kind: "agent", id: proposal.proposal!.id, agentId: proposal.agentId, action: "discard" });
		geral = await room(demo);
		expect(geral.cards.some((card) => card.kind === "agentProposal")).toBe(false);
		// 👎 on the Reviewer's answer stays, and a rule is kept.
		const answer = geral.lines.find((line) => line.speaker === "agent")!;
		await call(demo, "POST", "/api/v1/mobile/room/reactions", { sessionId: answer.sessionId, lineKey: answer.key, reaction: "down" });
		geral = await room(demo);
		expect(geral.lines.find((line) => line.key === answer.key)?.reaction).toBe("down");
		expect(await call(demo, "POST", "/api/v1/mobile/room/rules", { agentId: answer.agentId, body: "sempre cite o PR" })).toMatchObject({ ok: true });
		demo.dispose();
	});
});
