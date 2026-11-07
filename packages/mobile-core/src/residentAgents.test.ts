import { describe, expect, it } from "vitest";
import {
	isAgentHome,
	parseResidentAgents,
	relativeTime,
	residentMascotSvg,
	residentNews,
	residentStateLabel,
} from "./residentAgents.ts";
import { liveSessions } from "./triage.ts";

const answer = {
	snapshotId: "abc",
	agents: [
		{
			id: "rev",
			name: "Revisor",
			preset: "reviewer",
			avatar: { color: "blue", arms: 3, eyes: "round" },
			state: "watching",
			conversationSessionId: "s-home",
			last: { sessionId: "s-home", title: "Conversa", at: "2026-11-07T10:00:00Z", inConversation: true },
		},
		{ id: "res", name: "Pesquisador", avatar: {}, state: "mystery", conversationSessionId: null, last: null },
		{ name: "sem id" },
	],
};

describe("resident agents", () => {
	it("reads the agents and an unchanged answer only for the snapshot asked about", () => {
		const read = parseResidentAgents(answer, null);
		expect(read?.unchanged).toBe(false);
		if (!read || read.unchanged) throw new Error("expected agents");
		expect(read.agents.map((agent) => agent.id)).toEqual(["rev", "res"]);
		expect(read.agents[1].state).toBe("asleep");
		expect(read.agents[1].avatar.arms).toBe(3);
		expect(parseResidentAgents({ unchanged: true, snapshotId: "abc" }, "abc")).toEqual({ unchanged: true, snapshotId: "abc" });
		expect(parseResidentAgents({ unchanged: true, snapshotId: "abc" }, "old")).toBeNull();
		expect(parseResidentAgents(null, null)).toBeNull();
	});

	it("says what it last did, or how to start talking", () => {
		const read = parseResidentAgents(answer, null);
		if (!read || read.unchanged) throw new Error("expected agents");
		const now = Date.parse("2026-11-07T10:05:00Z");
		expect(residentNews(read.agents[0], now)).toBe("Respondeu na conversa · há 5 min");
		expect(residentNews(read.agents[1], now)).toBe("Comece a conversa no Mac");
		expect(relativeTime("2026-11-06T09:00:00Z", now)).toBe("ontem");
		expect(residentStateLabel("watching")).toBe("De olho");
	});

	it("draws the desktop's octopus for each state", () => {
		const avatar = { color: "violet", arms: 9, eyes: "round", shape: "octopus", accessory: "none" };
		const asleep = residentMascotSvg(avatar, "asleep");
		expect(asleep).toContain('opacity="0.6"');
		expect(asleep).toContain("M78 14h8");
		expect(asleep.match(/y="56"/g)?.length).toBe(5);
		expect(residentMascotSvg(avatar, "needsYou")).toContain("#F2A93B");
		expect(residentMascotSvg(avatar, "watching")).not.toEqual(residentMascotSvg(avatar, "working"));
		const drop = residentMascotSvg({ ...avatar, shape: "drop", accessory: "magnifier" }, "needsYou");
		expect(drop).toContain("M50 8C64");
		expect(drop).toContain('translate(0 16)');
		expect(drop).not.toContain("rotate(24 84 52)");
		expect(residentMascotSvg({ ...avatar, accessory: "glasses" }, "watching")).toContain('r="10.5"');
		const parsed = parseResidentAgents(answer, null);
		if (!parsed || parsed.unchanged) throw new Error("expected agents");
		expect(parsed.agents[0].avatar).toMatchObject({ shape: "octopus", accessory: "none" });
	});

	it("keeps the agents' conversations out of the task lists", () => {
		expect(isAgentHome("dcc-agent-rev")).toBe(true);
		expect(isAgentHome("dcc-agent-")).toBe(false);
		expect(isAgentHome("loja")).toBe(false);
		const sessions = [
			{ sessionId: "a", projectId: "dcc-agent-rev", workspaceId: "w1" },
			{ sessionId: "b", projectId: "loja", workspaceId: "w2" },
		] as Parameters<typeof liveSessions>[0];
		expect(liveSessions(sessions, []).map((s) => s.sessionId)).toEqual(["b"]);
	});
});
