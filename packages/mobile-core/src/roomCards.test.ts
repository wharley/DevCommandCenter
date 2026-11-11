import { describe, expect, it } from "vitest";
import {
	parseRoomCards,
	roomCardDetail,
	roomCardText,
	roomTriggerLabel,
	roomVerdictLabel,
	tokensShort,
	type RoomCard,
} from "./roomCards.ts";
import { parseRoom, roomItems, type RoomLine } from "./teamRoom.ts";

const result = {
	key: "done:s-1:2026-11-10T10:00:00Z",
	kind: "taskResult",
	agentId: "dev",
	title: "Checkout",
	at: "2026-11-10T10:00:00Z",
	unread: true,
	trigger: "ciFailed",
	verdict: null,
	workspaceId: "w-1",
	sessionId: "s-1",
	prId: null,
	actions: ["open", "delivery"],
};

describe("room cards", () => {
	it("read the desktop's cards and drop what they cannot use", () => {
		const cards = parseRoomCards([
			result,
			{ ...result, key: "pr:1", kind: "prReview", verdict: "approve", prId: "pr-1", actions: ["open", "fly"] },
			{
				key: "check:k",
				kind: "checkIn",
				agentId: null,
				title: "Checkout",
				at: "2026-11-10T11:00:00Z",
				unread: true,
				actions: ["open"],
				checkIn: { kind: "taskStalled", place: "app", url: null, minutes: 20 },
			},
			{ key: "limit:rev", kind: "limit", agentId: "rev", title: "Revisor", at: null, limit: 2_000_000, actions: ["continue"] },
			{
				key: "proposal:p",
				kind: "agentProposal",
				agentId: "dev",
				title: "QA",
				at: "2026-11-10T08:00:00Z",
				actions: ["create", "discard"],
				proposal: { id: "p", name: "QA", role: "Testa.", avatar: { color: "violet" }, projects: [], firstRequest: "teste o login" },
			},
			// No agent, an unknown kind, a proposal with no id, not an object.
			{ ...result, key: "x", agentId: null },
			{ ...result, key: "y", kind: "party" },
			{ key: "z", kind: "routineProposal", agentId: "dev", routine: {} },
			"nope",
		]);
		expect(cards.map((card) => card.key)).toEqual(["done:s-1:2026-11-10T10:00:00Z", "pr:1", "check:k", "limit:rev", "proposal:p"]);
		expect(cards[1]!.verdict).toBe("approve");
		expect(cards[1]!.actions).toEqual(["open"]);
		expect(cards[4]!.proposal!.avatar.shape).toBe("octopus");
		expect(parseRoomCards(undefined)).toEqual([]);
	});

	it("speak the desktop's words", () => {
		const [done, review, check, limit, proposal] = parseRoomCards([
			result,
			{ ...result, key: "pr", kind: "prReview", title: "app#1014 · Pagamentos", verdict: "approve" },
			{ key: "c", kind: "checkIn", agentId: null, title: "Checkout", checkIn: { kind: "taskStalled", minutes: 20 } },
			{ key: "l", kind: "limit", agentId: "rev", limit: 1_000_000 },
			{ key: "p", kind: "agentProposal", agentId: "dev", proposal: { id: "p", role: "Testa.", projects: ["app"], firstRequest: "teste o login" } },
		]) as RoomCard[];
		expect(roomCardText(done!, "Dev")).toBe("Dev terminou em Checkout");
		expect(roomTriggerLabel(done!.trigger)).toBe("Gatilho · CI falhou");
		expect(roomTriggerLabel("unknown")).toBeNull();
		expect(roomCardText(review!, "Revisor")).toBe("Revisor revisou app#1014 · Pagamentos");
		expect(roomVerdictLabel(review!.verdict!)).toBe("Pode aprovar");
		expect(roomCardText(check!, "")).toBe("Checkout está sem sinal há 20 min");
		expect(roomCardText(limit!, "Revisor")).toBe("Revisor chegou ao limite do mês (1M tokens)");
		expect(roomCardText(proposal!, "Dev")).toBe("Dev propõe um agente");
		expect(roomCardDetail(proposal!)).toEqual(["Testa.", "Em: app", "Primeiro pedido: teste o login"]);
		expect([tokensShort(2_500_000), tokensShort(500_000), tokensShort(900)]).toEqual(["2.5M", "500k", "900"]);
	});

	it("sit among the lines at their time, DCC's own after everything", () => {
		const line = (key: string, at: string): RoomLine => ({
			key,
			agentId: "dev",
			sessionId: "s",
			speaker: "agent",
			text: key,
			at,
			receipt: null,
			steps: null,
			tasks: [],
			wake: null,
			turnId: null,
			handoff: null,
			reaction: null,
		});
		const cards = parseRoomCards([
			{ key: "limit", kind: "limit", agentId: "rev", at: null },
			{ ...result, key: "card", at: "2026-11-10T10:01:00Z" },
		]);
		const now = new Date("2026-11-10T15:00:00Z");
		const items = roomItems([line("a", "2026-11-10T10:00:00Z"), line("b", "2026-11-10T10:02:00Z")], now, cards);
		expect(items.filter((item) => item.type !== "day").map((item) => (item.type === "card" ? item.card.key : `${item.line.key}${item.first ? "*" : ""}`))).toEqual([
			"a*",
			"card",
			// A card between two answers opens the next one's group again.
			"b*",
			"limit",
		]);
	});

	it("come with the room, and a reaction with its answer", () => {
		const room = parseRoom(
			{
				snapshotId: "s",
				lines: [{ key: "s:t", agentId: "dev", sessionId: "s", speaker: "agent", text: "Pronto", at: "2026-11-10T10:00:00Z", reaction: "down" }],
				cards: [result],
			},
			null,
		);
		expect(room && !room.unchanged && room.cards.map((card) => card.key)).toEqual([result.key]);
		expect(room && !room.unchanged && room.lines[0]!.reaction).toBe("down");
		const old = parseRoom({ snapshotId: "s", lines: [] }, null);
		expect(old && !old.unchanged && old.cards).toEqual([]);
	});
});
