import { describe, expect, it } from "vitest";
import { handoffHeading, matchingAgents, parseRoom, parseRoomSent, roomReasonLabel, roomReceiptLabel } from "./teamRoom.ts";

describe("team room", () => {
	it("reads the lines and an unchanged answer only for the snapshot asked about", () => {
		const read = parseRoom(
			{
				snapshotId: "abc",
				lines: [
					{ key: "s:1", agentId: "rev", sessionId: "s", speaker: "person", text: "revisa o PR", at: "t1", receipt: null },
					{ key: "s:t", agentId: "rev", sessionId: "s", speaker: "agent", text: "Tudo certo.", at: "t2", receipt: { seconds: 42, tokens: 1200 } },
					{ key: "x", agentId: "rev", sessionId: "s", speaker: "agent" },
				],
			},
			null,
		);
		if (!read || read.unchanged) throw new Error("expected lines");
		expect(read.lines.map((line) => line.speaker)).toEqual(["person", "agent"]);
		expect(roomReceiptLabel(read.lines[1]!.receipt)).toBe("42 s · 1.200 tokens");
		expect(parseRoom({ unchanged: true, snapshotId: "abc" }, "abc")).toEqual({ unchanged: true, snapshotId: "abc" });
		expect(parseRoom({ unchanged: true, snapshotId: "abc" }, "old")).toBeNull();
		expect(parseRoom({ snapshotId: "abc" }, null)).toBeNull();
	});

	it("says where a message went and shows a passed one by its heading", () => {
		const sent = parseRoomSent({ agentId: "rev", agentName: "Revisor", reason: "role", sessionId: "s" });
		expect(sent && roomReasonLabel(sent.reason)).toBe("pelo papel");
		expect(parseRoomSent({ agentId: "rev" })).toBeNull();
		expect(handoffHeading("↪ Revisor → QA: roda os testes\n\nWhat Revisor said…")).toBe("↪ Revisor → QA: roda os testes");
		expect(handoffHeading("oi")).toBeNull();
	});

	it("finds the team by name as the @ is typed", () => {
		const team = [{ name: "Revisor" }, { name: "QA Mobile" }, { name: "Pesquisa" }];
		expect(matchingAgents(team, "").map((agent) => agent.name)).toEqual(["Revisor", "QA Mobile", "Pesquisa"]);
		expect(matchingAgents(team, "is").map((agent) => agent.name)).toEqual(["Revisor", "Pesquisa"]);
		expect(matchingAgents(team, "qa").map((agent) => agent.name)).toEqual(["QA Mobile"]);
	});
});
