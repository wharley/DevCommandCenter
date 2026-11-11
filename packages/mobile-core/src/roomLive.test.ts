import { describe, expect, it } from "vitest";
import {
	handoffLabel,
	keepUnchanged,
	linesSince,
	liveClock,
	liveLabel,
	liveSeconds,
	liveTitle,
	moreStepsLabel,
	newLinesLabel,
	parseRoomLive,
	parseRoomQueued,
	parseRoomSteps,
	parseRoomStopped,
	queuedLabel,
	stepText,
	stoppedLabel,
	type RoomLive,
} from "./roomLive.ts";
import { parseRoom, parseRoomSent } from "./teamRoom.ts";

const at = (seconds: number) => new Date(Date.parse("2026-11-10T10:00:00Z") + seconds * 1000).getTime();

describe("room at work", () => {
	it("reads an agent's turn at work and words what it does now", () => {
		const [live] = parseRoomLive([
			{
				kind: "conversation",
				agentId: "dev",
				sessionId: "s",
				turnId: "t",
				startedAt: "2026-11-10T10:00:00Z",
				doing: "tool",
				steps: [
					{ kind: "read", target: "src/cache.rs", state: "done" },
					{ kind: "command", target: "yarn test", state: "failed" },
					{ kind: "read", target: "src/cache.rs", state: "running" },
				],
				earlier: 4,
				text: "",
			},
			{ agentId: "" },
		]);
		expect(live).toBeDefined();
		const turn = live as RoomLive;
		expect(turn.key).toBe("s");
		expect(turn.earlier).toBe(4);
		expect(liveLabel(turn, liveSeconds(turn, at(84)))).toBe("Lendo src/cache.rs");
		expect(liveClock(liveSeconds(turn, at(84)))).toBe("1:24");
		expect(liveClock(3725)).toBe("1:02:05");
		expect(liveClock(-3)).toBe("0:00");
		const idle = { ...turn, steps: [], doing: "thinking" as const };
		expect(liveLabel(idle, 5)).toBe("Pensando…");
		expect(liveLabel({ ...idle, doing: "reading" }, 5)).toBe("Lendo o pedido…");
		expect(liveLabel(idle, 75)).toBe("Ainda trabalhando: tarefas longas levam um tempo");
		expect(liveLabel({ ...idle, text: "Achei o bug" }, 75)).toBe("Escrevendo…");
		expect(stepText({ kind: "command", target: "yarn test" }, false)).toBe("Rodou yarn test");
		expect(stepText({ kind: "plan", target: null }, true)).toBe("Atualizando o plano");
		expect(moreStepsLabel(4)).toBe("+4 passos");
		expect(liveTitle(turn, "Dev")).toBe("Dev");
	});

	it("reads a task at work under the room, with its name and no text", () => {
		const [task] = parseRoomLive([
			{ kind: "task", agentId: "dev", sessionId: "c", turnId: "t", doing: "thinking", steps: [], earlier: 0, task: { title: "Corrigir domínio", workspaceId: "w" } },
		]);
		expect(task?.kind).toBe("task");
		expect(task?.text).toBe("");
		expect(liveTitle(task!, "Dev")).toBe("Dev · “Corrigir domínio”");
	});

	it("offers a stop again only with the person's prompt", () => {
		const stopped = parseRoomStopped([
			{ agentId: "dev", sessionId: "s", turnId: "t", reason: "exit 1", retry: { sessionId: "s", prompt: "corrige", turnId: "t" } },
			{ agentId: "qa", sessionId: "q", turnId: "u", reason: null, retry: null },
		]);
		expect(stopped[0]?.retry).toEqual({ sessionId: "s", prompt: "corrige", turnId: "t" });
		expect(stopped[1]?.retry).toBeNull();
		expect(stoppedLabel(stopped[0]!, "Dev")).toBe("Dev parou antes de terminar · exit 1");
		expect(stoppedLabel(stopped[1]!, "QA")).toBe("QA parou antes de terminar");
	});

	it("reads the queue and says when it goes", () => {
		const queued = parseRoomQueued([{ id: "q1", agentId: "dev", sessionId: "s", text: "e o deploy?", at: "x" }, { id: "q2" }]);
		expect(queued).toHaveLength(1);
		expect(queuedLabel("Dev")).toBe("Na fila · sai quando o Dev terminar");
		expect(parseRoomSent({ agentId: "dev", agentName: "Dev", sessionId: "s", reason: "mention", queued: true })?.queued).toBe(true);
		expect(parseRoomSent({ agentId: "dev", agentName: "Dev", sessionId: "s" })?.queued).toBe(false);
	});

	it("reads an answer's handoff and its steps", () => {
		const read = parseRoom(
			{
				snapshotId: "a",
				linesId: "l",
				lines: [
					{
						key: "s:t",
						agentId: "dev",
						sessionId: "s",
						speaker: "agent",
						text: "Feito.\n@QA testa?",
						turnId: "t",
						handoff: { to: "qa", toName: "QA", request: "@QA testa?", state: "pending" },
					},
				],
				approvals: [{ sessionId: "s", requestId: "r", agentId: "dev", toolName: "slack_search", mayAlwaysAllow: true }],
			},
			null,
		);
		expect(read && !read.unchanged && read.lines[0]?.handoff?.toName).toBe("QA");
		expect(read && !read.unchanged && read.lines[0]?.turnId).toBe("t");
		expect(read && !read.unchanged && read.approvals[0]?.mayAlwaysAllow).toBe(true);
		expect(read && !read.unchanged && read.linesId).toBe("l");
		expect(handoffLabel({ to: "qa", toName: "QA", request: "", state: "pending" }, "Dev")).toBe("Dev sugere passar a vez para QA");
		expect(handoffLabel({ to: "qa", toName: "QA", request: "", state: "passed" }, "Dev")).toBe("Passado para QA");
		expect(handoffLabel({ to: "qa", toName: "QA", request: "", state: "auto" }, "Dev")).toBe("Passado automaticamente para QA");
		expect(parseRoomSteps({ steps: [{ kind: "read", target: "a.rs", failed: false }, { kind: "zzz", failed: true }], more: 2 })).toEqual({
			steps: [
				{ kind: "read", target: "a.rs", failed: false },
				{ kind: "other", target: null, failed: true },
			],
			more: 2,
		});
		expect(parseRoomSteps({})).toBeNull();
	});

	it("keeps the lines held when only the turns at work moved", () => {
		const read = parseRoom({ snapshotId: "b", linesId: "l", linesUnchanged: true, live: [] }, "a");
		expect(read && !read.unchanged && read.linesUnchanged).toBe(true);
		expect(read && !read.unchanged && read.lines).toEqual([]);
		// An older desktop: no lines, no answer.
		expect(parseRoom({ snapshotId: "b" }, null)).toBeNull();
	});

	it("keeps each line that did not change, and the list itself when none did", () => {
		const a = { key: "a", text: "oi" };
		const b = { key: "b", text: "tudo" };
		const held = [a, b];
		expect(keepUnchanged(held, [{ key: "a", text: "oi" }, { key: "b", text: "tudo" }], (line) => line.key)).toBe(held);
		const grown = keepUnchanged(held, [{ key: "a", text: "oi" }, { key: "b", text: "tudo" }, { key: "c", text: "novo" }], (line) => line.key);
		expect(grown).not.toBe(held);
		expect(grown[0]).toBe(a);
		expect(grown[1]).toBe(b);
		const edited = keepUnchanged(held, [{ key: "a", text: "oi" }, { key: "b", text: "mudou" }], (line) => line.key);
		expect(edited[0]).toBe(a);
		expect(edited[1]).not.toBe(b);
		// Fewer lines: a new list, the same objects.
		const shorter = keepUnchanged(held, [{ key: "b", text: "tudo" }], (line) => line.key);
		expect(shorter).toEqual([b]);
		expect(shorter[0]).toBe(b);
	});

	it("counts the lines that came since the newest one seen", () => {
		const lines = [{ key: "a" }, { key: "b" }, { key: "c" }];
		expect(linesSince(lines, "a")).toBe(2);
		expect(linesSince(lines, "c")).toBe(0);
		expect(linesSince(lines, null)).toBe(0);
		expect(linesSince(lines, "gone")).toBe(0);
		expect(newLinesLabel(1)).toBe("1 nova");
		expect(newLinesLabel(3)).toBe("3 novas");
	});
});
