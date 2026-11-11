import { describe, expect, it } from "vitest";
import {
	approvalTitle,
	handoffHeading,
	matchingAgents,
	questionTitle,
	parseRoom,
	parseRoomSent,
	roomDayLabel,
	roomItems,
	roomReasonLabel,
	roomReceiptLabel,
	roomStepsLabel,
	roomTaskStateLabel,
	roomWakeLabel,
	splitMentions,
	type RoomLine,
} from "./teamRoom.ts";

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

	it("reads the approvals waiting in a room and who works there", () => {
		const read = parseRoom(
			{
				snapshotId: "abc",
				lines: [],
				approvals: [
					{ sessionId: "s", requestId: "r", agentId: "dev", agentName: "Dev", toolName: "Bash", summary: "yarn test" },
					{ sessionId: "s", agentId: "dev" },
				],
				working: ["dev", 7],
			},
			null,
		);
		if (!read || read.unchanged) throw new Error("expected a room");
		expect(read.approvals).toHaveLength(1);
		expect(approvalTitle(read.approvals[0]!)).toBe("Dev quer usar Bash");
		expect(approvalTitle({ ...read.approvals[0]!, toolName: "mcp__slack__send_message" })).toBe("Dev quer usar slack · send_message");
		expect(read.working).toEqual(["dev"]);
		// An older desktop says neither.
		const old = parseRoom({ snapshotId: "abc", lines: [] }, null);
		expect(old && !old.unchanged && [old.approvals, old.working]).toEqual([[], null]);
	});

	it("reads the questions waiting in a room in the thread's shape", () => {
		const read = parseRoom(
			{
				snapshotId: "abc",
				lines: [],
				questions: [
					{
						sessionId: "s",
						requestId: "q-1",
						agentId: "dev",
						agentName: "Dev",
						questions: [
							{ id: "env", header: "Destino", question: "Qual ambiente?", options: [{ label: "staging", description: "Seguro" }, { label: 7 }] },
							{ id: "blank", header: "", question: "  ", options: [] },
							{ header: "Quando", question: "Quando publico?" },
						],
					},
					{ sessionId: "s", requestId: "q-2", agentId: "dev", questions: [] },
					{ requestId: "q-3", agentId: "dev", questions: [{ question: "Sem sessão?" }] },
				],
			},
			null,
		);
		if (!read || read.unchanged) throw new Error("expected a room");
		expect(read.questions).toHaveLength(1);
		const question = read.questions[0]!;
		expect(questionTitle(question)).toBe("Dev pergunta");
		expect(question.questions).toEqual([
			{ id: "env", header: "Destino", question: "Qual ambiente?", options: [{ label: "staging", description: "Seguro" }] },
			{ id: "Quando publico?", header: "Quando", question: "Quando publico?", options: [] },
		]);
		expect(questionTitle({ ...question, agentName: "" })).toBe("O agente pergunta");
		// An older desktop serves none.
		const old = parseRoom({ snapshotId: "abc", lines: [] }, null);
		expect(old && !old.unchanged && old.questions).toEqual([]);
	});

	it("puts a pill at each new day and one face on answers close together", () => {
		const line = (key: string, speaker: RoomLine["speaker"], agentId: string, at: Date): RoomLine => ({ key, agentId, sessionId: "s", speaker, text: key, at: at.toISOString(), receipt: null, steps: null, tasks: [], wake: null, turnId: null, handoff: null });
		const now = new Date(2026, 10, 10, 15, 0);
		const items = roomItems(
			[
				line("a", "agent", "dev", new Date(2026, 10, 9, 10, 0)),
				line("b", "person", "dev", new Date(2026, 10, 10, 9, 0)),
				line("c", "agent", "dev", new Date(2026, 10, 10, 9, 1)),
				line("d", "agent", "dev", new Date(2026, 10, 10, 9, 4)),
				line("e", "agent", "qa", new Date(2026, 10, 10, 9, 5)),
				line("f", "agent", "dev", new Date(2026, 10, 10, 9, 30)),
			],
			now,
		);
		expect(items.map((item) => (item.type === "day" ? item.label : `${item.line.key}${item.first ? "*" : ""}`))).toEqual(["Ontem", "a*", "Hoje", "b*", "c*", "d", "e*", "f*"]);
		expect(roomDayLabel(new Date(2026, 9, 3, 9, 0).toISOString(), now)).toBe("3 de out.");
	});

	it("finds each @Agent as the router does, never inside an e-mail or a path", () => {
		const team = [
			{ id: "dev", name: "Dev" },
			{ id: "devops", name: "Dev Ops" },
			{ id: "rev", name: "Revisão" },
		];
		const ids = (body: string) => splitMentions(body, team).map((part) => (part.agentId ? `[${part.text}:${part.agentId}]` : part.text));
		expect(ids("@dev corrige isso")).toEqual(["[@dev:dev]", " corrige isso"]);
		expect(ids("chama o @Dev Ops e a (@revisao)")).toEqual(["chama o ", "[@Dev Ops:devops]", " e a (", "[@revisao:rev]", ")"]);
		expect(ids("fala com a@dev.com e veja @/tmp/dev.png, @Devs")).toEqual(["fala com a@dev.com e veja @/tmp/dev.png, @Devs"]);
		expect(ids("sem ninguém")).toEqual(["sem ninguém"]);
	});

	it("reads what a turn did and the task it reports", () => {
		const read = parseRoom(
			{
				snapshotId: "abc",
				lines: [
					{
						key: "s:t", agentId: "dev", sessionId: "s", speaker: "agent", text: "Feito.", at: "t", receipt: { seconds: 240, tokens: 10 },
						steps: { seconds: 240, tools: 4, failures: 1, counts: { command: 1, read: 2, plan: 1, other: 0 } },
						task: { delegationId: "d", workspaceId: "w", sessionId: "c", title: "Corrigir domínio", state: "turnEnded", files: 2, additions: 50, deletions: 4 },
						reports: [
							{ delegationId: "d", workspaceId: "w", sessionId: "c", title: "Corrigir domínio", state: "turnEnded", files: 2, additions: 50, deletions: 4 },
							{ delegationId: "e", title: "sem tarefa" },
						],
					},
					// A turn with no tool: the raw empty counts say nothing.
					{ key: "s:u", agentId: "dev", sessionId: "s", speaker: "agent", text: "Oi.", at: "t", receipt: null, steps: { tools: 0, failures: 0, counts: {} }, tasks: [] },
				],
			},
			null,
		);
		if (!read || read.unchanged) throw new Error("expected lines");
		const [worked, plain] = read.lines;
		expect(roomStepsLabel(worked!.steps!)).toBe("Trabalhou 4 min · rodou 1 comando · leu 2 arquivos · 1 falhou");
		expect(worked!.tasks).toHaveLength(1);
		expect(worked!.tasks[0]).toMatchObject({ workspaceId: "w", sessionId: "c", additions: 50 });
		expect(roomTaskStateLabel(worked!.tasks[0]!.state)).toBe("Terminou o turno");
		expect(plain!.steps).toBeNull();
		expect(plain!.wake).toBeNull();
		const woke = parseRoom({ snapshotId: "a", lines: [{ key: "k", agentId: "sup", sessionId: "s", speaker: "person", text: "[DCC] Source …", at: "t", kind: "source", about: "menções no Slack (Slack)" }] }, null);
		expect(woke && !woke.unchanged && roomWakeLabel(woke.lines[0]!.wake!)).toBe("Fonte · menções no Slack (Slack)");
		expect(plain!.tasks).toEqual([]);
	});
});
