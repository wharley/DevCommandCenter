/**
 * The rooms of the demo desktop (`demo.ts`): three resident agents, Geral
 * and two project rooms with a few lines, one approval and one question
 * waiting in a room and an agent that answers what the person writes. Same paths and shapes
 * as the real desktop (`/api/v1/mobile/rooms`, `/mobile/room`, `/mobile/agents`).
 *
 * An agent written to works live (`live`: its steps, then its answer
 * streaming) before it answers; writing again while it works queues the
 * message; "Parar" stops it and "Tentar de novo" sends the prompt again;
 * the Reviewer's answer in Geral suggests passing the turn to Dev; Dev's
 * task in loja-online runs live under the room; each answer's steps open.
 * The turns are read from the clock on each read, no timer drives them.
 *
 * The rooms carry the desktop's cards too: Dev's finished task in
 * loja-online (woken by "CI falhou"), the Reviewer's pull request review
 * with "Pode aprovar", a check-in and Dev's proposal of a QA agent in Geral
 * ("Criar" / "Descartar"); 👍 / 👎 under an answer stay, and a rule kept
 * after a 👎 is accepted.
 */
import { ApiError } from "./types";

type Line = {
	key: string;
	agentId: string;
	sessionId: string;
	speaker: "person" | "agent";
	text: string;
	at: string;
	receipt: { seconds: number; tokens: number } | null;
	steps?: unknown;
	task?: unknown;
	turnId?: string;
	handoff?: { to: string; toName: string; request: string; state: "pending" | "passed" | "auto" };
	reaction?: "up" | "down" | null;
};

/** A card as the desktop serves it (`cards` on `/mobile/room`), with its room. */
type Card = { room: string } & Record<string, unknown>;

/** An agent at work in a room, read from the clock. */
type Turn = { room: string; agentId: string; sessionId: string; turnId: string; startedAt: number; prompt: string };

/** What a demo turn does, by milliseconds since it started. */
const SCRIPT: Array<{ at: number; end: number; kind: string; target: string }> = [
	{ at: 300, end: 1800, kind: "read", target: "src/checkout/gateway.ts" },
	{ at: 1800, end: 2900, kind: "search", target: "PAYMENT_FLAG" },
	{ at: 2900, end: 4600, kind: "command", target: "yarn test checkout" },
	{ at: 4600, end: 5600, kind: "edit", target: "src/checkout/gateway.ts" },
];
const WRITE_AT = 5800;
const TURN_MS = 8600;
const ANSWER =
	"Anotado. No modo demonstração eu leio, rodo os testes e respondo isto; com o DCC no computador, a resposta vem do agente de verdade.";
/** The steps an answer lists when opened. */
const STEPS = [
	{ kind: "read", target: "src/checkout/gateway.ts", failed: false },
	{ kind: "search", target: "PAYMENT_FLAG", failed: false },
	{ kind: "command", target: "yarn test checkout", failed: true },
	{ kind: "edit", target: "src/checkout/gateway.ts", failed: false },
	{ kind: "command", target: "yarn test checkout", failed: false },
];

const AGENTS = [
	{ id: "demo-dev", name: "Dev", preset: null, avatar: { color: "emerald", arms: 4, eyes: "round", shape: "octopus", accessory: "cap" } },
	{ id: "demo-rev", name: "Revisor", preset: "reviewer", avatar: { color: "violet", arms: 3, eyes: "round", shape: "ghost", accessory: "glasses" } },
	{ id: "demo-sup", name: "Suporte", preset: null, avatar: { color: "sky", arms: 3, eyes: "round", shape: "drop", accessory: "antenna" } },
];

const ROOMS: Array<{ id: string | null; name: string }> = [
	{ id: null, name: "Geral" },
	{ id: "loja", name: "loja-online" },
	{ id: "site", name: "site-institucional" },
];

/** The session a demo line opens: one of the demo's finished threads. */
const THREAD = "demo-s4";
const APPROVAL_SESSION = "demo-room-dev";
const QUESTION_SESSION = "demo-room-rev";
/** The Reviewer's question in the site's room, as the thread's card reads it. */
const QUESTION = [
	{
		id: "fix",
		header: "Link do passo 3",
		question: "Corrijo o link antigo agora ou deixo só o comentário?",
		options: [
			{ label: "Corrigir agora", description: "Faço o commit na tarefa do PR 18" },
			{ label: "Só comentar", description: "Quem abriu o PR corrige" },
		],
	},
	{ id: "merge", header: "Depois", question: "Posso aprovar o PR quando o check passar?", options: [] },
];

export class DemoRooms {
	private readonly lines = new Map<string, Line[]>();
	private readonly working = new Map<string, Set<string>>();
	private approvalOpen = true;
	private questionOpen = true;
	private version = 0;
	private counter = 0;
	private readonly turns = new Map<string, Turn>();
	private queue: Array<{ id: string; room: string; agentId: string; sessionId: string; text: string; at: string }> = [];
	private stopped: Array<{ room: string; agentId: string; sessionId: string; turnId: string; reason: string | null; prompt: string | null }> = [];
	/** When Dev's task in loja-online started, for its live row. */
	private readonly taskStartedAt: number;
	private cards: Card[] = [];

	constructor(
		now: number,
		/** Tells the open screens something moved. */
		private readonly notify: () => void,
		private readonly later: (run: () => void, ms: number) => void,
	) {
		const ago = (minutes: number) => new Date(now - minutes * 60_000).toISOString();
		const line = (room: string, agentId: string, speaker: Line["speaker"], text: string, minutes: number, receipt: Line["receipt"] = null) => {
			const list = this.lines.get(room) ?? [];
			list.push({ key: `demo-${room}-${list.length}`, agentId, sessionId: THREAD, speaker, text, at: ago(minutes), receipt });
			this.lines.set(room, list);
		};
		line("geral", "demo-sup", "person", "@Suporte o que chegou no Slack hoje?", 60 * 26);
		line("geral", "demo-sup", "agent", "Duas menções novas em #pagamentos: um cliente sem recibo e uma dúvida sobre parcelamento. Nenhuma pede código.", 60 * 26 - 2, { seconds: 38, tokens: 5200 });
		line("geral", "demo-rev", "person", "@Revisor olha o PR 18 do site", 95);
		line("geral", "demo-rev", "agent", "Revisei o PR 18: o guia está claro. Um bloqueante — o link do passo 3 aponta para a página antiga. Deixei o comentário como rascunho.", 92, { seconds: 71, tokens: 9100 });
		line("loja", "demo-dev", "person", "@Dev migra o checkout para o novo gateway", 14);
		line("loja", "demo-dev", "agent", "Comecei pela camada de pagamento. Abri a tarefa **checkout-v2** para isso; o gateway antigo fica atrás de uma flag. Referência: [doc do gateway](https://docs.example.com/gateway/v2) e https://status.example.com/incidents/2026-11-11-checkout-latency.\n\n@Revisor quando terminar, revisa o PR?", 12, { seconds: 95, tokens: 14300 });
		const opened = this.lines.get("loja")!.at(-1)!;
		opened.steps = { seconds: 95, tools: 6, failures: 0, counts: { read: 4, command: 1, edit: 1 } };
		opened.task = { delegationId: "demo-d1", workspaceId: "demo-w1", sessionId: "demo-s1", title: "checkout-v2", state: "working", files: 3, additions: 78, deletions: 14 };
		line("site", "demo-rev", "agent", "O guia de onboarding tem um check de lint falhando. Sugiro corrigir antes de mesclar.", 60 * 5, { seconds: 22, tokens: 3100 });
		this.working.set("loja", new Set(["demo-dev"]));
		const reviewed = this.lines.get("geral")!.at(-1)!;
		reviewed.text += "\n\n@Dev pode corrigir o link do passo 3?";
		reviewed.turnId = "demo-turn-rev";
		reviewed.steps = { seconds: 71, tools: 5, failures: 1, counts: { read: 2, search: 1, command: 2 } };
		reviewed.handoff = { to: "demo-dev", toName: "Dev", request: "@Dev pode corrigir o link do passo 3?", state: "pending" };
		opened.turnId = "demo-turn-dev";
		// Suporte stopped in loja-online: "Tentar de novo" sends the prompt again.
		this.stopped.push({ room: "loja", agentId: "demo-sup", sessionId: "demo-live-loja-demo-sup", turnId: "demo-stop-1", reason: "O provedor encerrou a conexão", prompt: "@Suporte responde o cliente sem recibo" });
		this.taskStartedAt = now - 3 * 60_000;
		const card = (room: string, key: string, kind: string, minutes: number | null, fields: Record<string, unknown>) =>
			this.cards.push({
				room,
				key,
				kind,
				agentId: null,
				title: "",
				at: minutes === null ? null : ago(minutes),
				unread: false,
				trigger: null,
				verdict: null,
				workspaceId: null,
				sessionId: null,
				prId: null,
				actions: ["open"],
				...fields,
			});
		card("loja", "demo-card-done", "taskResult", 30, {
			agentId: "demo-dev",
			title: "corrigir-frete",
			unread: true,
			trigger: "ciFailed",
			workspaceId: "demo-w4",
			sessionId: THREAD,
			actions: ["open", "delivery"],
		});
		card("geral", "demo-card-review", "prReview", 90, {
			agentId: "demo-rev",
			title: "site#18 · Guia de onboarding",
			unread: true,
			verdict: "approve",
			sessionId: THREAD,
			prId: "demo-pr-18",
		});
		card("geral", "demo-card-check", "checkIn", 50, {
			title: "Atualizar SDK de pagamentos",
			unread: true,
			sessionId: THREAD,
			checkIn: { kind: "reviewRequested", place: "loja-online #42", url: null, minutes: 20 },
		});
		card("geral", "demo-card-proposal", "agentProposal", 20, {
			agentId: "demo-dev",
			title: "QA",
			actions: ["create", "discard"],
			proposal: {
				id: "demo-proposal-qa",
				name: "QA",
				role: "Testa cada entrega no navegador antes do PR e conta o que quebrou, com o passo a passo.",
				avatar: { color: "amber", arms: 3, eyes: "round", shape: "block", accessory: "magnifier" },
				projects: ["loja-online"],
				firstRequest: "Teste o checkout-v2 no navegador",
			},
		});
	}

	/** 👍 / 👎 on an answer: kept on its line, as the desktop keeps it. */
	private react(body: unknown) {
		const input = (body ?? {}) as { lineKey?: unknown; reaction?: unknown };
		const line = [...this.lines.values()].flat().find((item) => item.key === input.lineKey);
		if (!line || line.speaker !== "agent") throw new ApiError(404, "conversation not found");
		line.reaction = input.reaction === "up" || input.reaction === "down" ? input.reaction : null;
		this.version += 1;
		this.notify();
		return { ok: true, reaction: line.reaction };
	}

	/** "Criar" / "Descartar" on a proposal, "Continuar mesmo assim" on a limit: the card goes. */
	private answerCard(match: (card: Card) => boolean) {
		const before = this.cards.length;
		this.cards = this.cards.filter((card) => !match(card));
		if (this.cards.length === before) throw new ApiError(404, "proposal not found");
		this.version += 1;
		this.notify();
		return { ok: true };
	}

	/** Ends the turns whose time is up: the answer, then the next message queued there. */
	private settle(now: number) {
		for (const [sessionId, turn] of [...this.turns]) {
			if (now - turn.startedAt < TURN_MS) continue;
			this.turns.delete(sessionId);
			this.say(turn.room, turn.agentId, "agent", ANSWER, { seconds: Math.round(TURN_MS / 1000), tokens: 620 }, sessionId, turn.turnId);
			this.working.get(turn.room)?.delete(turn.agentId);
			const next = this.queue.find((item) => item.sessionId === sessionId);
			if (next) {
				this.queue = this.queue.filter((item) => item !== next);
				this.say(next.room, next.agentId, "person", next.text);
				this.start(next.room, next.agentId, next.text);
			}
		}
	}

	private say(room: string, agentId: string, speaker: Line["speaker"], text: string, receipt: Line["receipt"] = null, sessionId = THREAD, turnId?: string) {
		const list = this.lines.get(room) ?? [];
		this.counter += 1;
		const line: Line = { key: `demo-${room}-new-${this.counter}`, agentId, sessionId, speaker, text, at: new Date().toISOString(), receipt };
		if (speaker === "agent") {
			line.turnId = turnId ?? `demo-turn-${this.counter}`;
			line.steps = { seconds: receipt?.seconds ?? null, tools: 5, failures: 1, counts: { read: 1, search: 1, command: 2, edit: 1 } };
		}
		list.push(line);
		this.lines.set(room, list);
		this.version += 1;
	}

	/** `agentId` starts working in `room`; the phone hears of it, and of its end. */
	private start(room: string, agentId: string, prompt: string) {
		const sessionId = `demo-live-${room}-${agentId}`;
		this.counter += 1;
		this.turns.set(sessionId, { room, agentId, sessionId, turnId: `demo-turn-live-${this.counter}`, startedAt: Date.now(), prompt });
		const set = this.working.get(room) ?? new Set<string>();
		set.add(agentId);
		this.working.set(room, set);
		this.version += 1;
		this.later(() => {
			this.settle(Date.now());
			this.notify();
		}, TURN_MS + 50);
	}

	/** An agent's turn at work, as the desktop serves it. */
	private liveOf(turn: Turn, now: number) {
		const elapsed = now - turn.startedAt;
		const steps = SCRIPT.filter((step) => step.at <= elapsed).map((step) => ({
			kind: step.kind,
			target: step.target,
			state: elapsed < step.end ? "running" : "done",
		}));
		const written = elapsed < WRITE_AT ? 0 : Math.ceil((ANSWER.length * (elapsed - WRITE_AT)) / (TURN_MS - WRITE_AT - 400));
		return {
			kind: "conversation",
			agentId: turn.agentId,
			sessionId: turn.sessionId,
			turnId: turn.turnId,
			startedAt: new Date(turn.startedAt).toISOString(),
			doing: steps.some((step) => step.state === "running") ? "tool" : steps.length === 0 ? "reading" : "thinking",
			steps,
			earlier: 0,
			text: ANSWER.slice(0, written),
		};
	}

	/** Dev's task in loja-online at work, its steps going round. */
	private taskLive(now: number) {
		const elapsed = (now - this.taskStartedAt) % (TURN_MS - 2000);
		const round = Math.floor((now - this.taskStartedAt) / (TURN_MS - 2000));
		return {
			kind: "task",
			agentId: "demo-dev",
			sessionId: "demo-s1",
			turnId: "demo-task-turn",
			startedAt: new Date(this.taskStartedAt).toISOString(),
			doing: "tool",
			steps: SCRIPT.filter((step) => step.at <= elapsed).map((step) => ({ kind: step.kind, target: step.target, state: elapsed < step.end ? "running" : "done" })),
			earlier: round * SCRIPT.length,
			task: { title: "checkout-v2", workspaceId: "demo-w1" },
		};
	}

	private roomRead(key: string, now: number) {
		const live: object[] = [...this.turns.values()].filter((turn) => turn.room === key).map((turn) => this.liveOf(turn, now));
		if (key === "loja") live.unshift(this.taskLive(now));
		return {
			live,
			stopped: this.stopped
				.filter((stop) => stop.room === key && !this.turns.has(stop.sessionId))
				.map((stop) => ({
					agentId: stop.agentId,
					sessionId: stop.sessionId,
					turnId: stop.turnId,
					reason: stop.reason,
					retry: stop.prompt ? { sessionId: stop.sessionId, prompt: stop.prompt, turnId: stop.turnId } : null,
				})),
			queued: this.queue.filter((item) => item.room === key).map(({ room: _room, ...item }) => item),
		};
	}

	private key(room: string | null): string {
		return room ?? "geral";
	}

	private snapshot(scope: string): string {
		return `demo-${scope}-${this.version}`;
	}

	private agents() {
		const busy = new Set([...this.working.values()].flatMap((set) => [...set]));
		return AGENTS.map((agent) => ({
			...agent,
			state:
				(this.approvalOpen && agent.id === "demo-dev") || (this.questionOpen && agent.id === "demo-rev")
					? "needsYou"
					: busy.has(agent.id) ? "working" : agent.id === "demo-sup" ? "watching" : "asleep",
			conversationSessionId: THREAD,
			last: null,
		}));
	}

	private rooms() {
		return ROOMS.map((room) => {
			const key = this.key(room.id);
			const last = this.lines.get(key)?.at(-1) ?? null;
			return {
				id: room.id,
				name: room.name,
				last: last
					? { agentId: last.agentId, agentName: AGENTS.find((a) => a.id === last.agentId)?.name ?? "", speaker: last.speaker, text: last.text.replace(/\s+/g, " ").slice(0, 140), at: last.at }
					: null,
				working: [...(this.working.get(key) ?? [])],
				needsYou: (this.approvalOpen && key === "loja") || (this.questionOpen && key === "site"),
				running: key === "loja",
				at: last?.at ?? null,
			};
		});
	}

	private send(body: unknown) {
		const input = (body ?? {}) as { text?: unknown; room?: unknown; sessionId?: unknown };
		const text = typeof input.text === "string" ? input.text.trim() : "";
		if (!text) throw new ApiError(400, "Mensagem vazia.");
		const key = this.key(typeof input.room === "string" && input.room ? input.room : null);
		// "Tentar de novo": the stopped conversation, past the rule.
		const retried = this.stopped.find((stop) => stop.sessionId === input.sessionId);
		const called = AGENTS.find((agent) => text.toLowerCase().includes(`@${agent.name.toLowerCase()}`));
		const last = [...(this.lines.get(key) ?? [])].reverse().find((line) => line.speaker === "agent");
		const agent = AGENTS.find((a) => a.id === retried?.agentId) ?? called ?? AGENTS.find((a) => a.id === last?.agentId) ?? AGENTS[1]!;
		if (retried) this.stopped = this.stopped.filter((stop) => stop !== retried);
		const sessionId = `demo-live-${key}-${agent.id}`;
		const reason = retried ? "continues" : called ? "mention" : last ? "continues" : "default";
		const room = key === "geral" ? null : key;
		if (this.turns.has(sessionId)) {
			// At work: the message waits for its turn to end.
			this.counter += 1;
			const id = `demo-queued-${this.counter}`;
			this.queue.push({ id, room: key, agentId: agent.id, sessionId, text, at: new Date().toISOString() });
			this.version += 1;
			this.notify();
			return { agentId: agent.id, agentName: agent.name, reason, sessionId, room, queued: true, queuedId: id };
		}
		this.say(key, agent.id, "person", text);
		this.start(key, agent.id, text);
		this.notify();
		return { agentId: agent.id, agentName: agent.name, reason, sessionId, room, queued: false, queuedId: null };
	}

	/** "Passar a vez" or "Ignorar" under the Reviewer's answer. */
	private handoff(body: unknown) {
		const input = (body ?? {}) as { key?: unknown; action?: unknown };
		const line = [...this.lines.values()].flat().find((item) => item.key === input.key);
		if (!line?.handoff || line.handoff.state !== "pending") throw new ApiError(409, "roomNoHandoff");
		const room = [...this.lines.entries()].find(([, list]) => list.includes(line))![0];
		const to = AGENTS.find((agent) => agent.id === line.handoff!.to)!;
		if (input.action === "ignore") {
			delete line.handoff;
			this.version += 1;
			this.notify();
			return { ignored: true };
		}
		line.handoff = { ...line.handoff, state: "passed" };
		const from = AGENTS.find((agent) => agent.id === line.agentId)?.name ?? "";
		this.say(room, to.id, "person", `↪ ${from} → ${to.name}: ${line.handoff.request}\n\nWhat ${from} said in the team room:\n${line.text}`);
		this.start(room, to.id, line.handoff.request);
		this.notify();
		return { agentId: to.id, agentName: to.name, reason: "mention", sessionId: `demo-live-${room}-${to.id}`, room: room === "geral" ? null : room, queued: false };
	}

	/** "Parar": the turn ends where it is, and may be tried again. */
	private abort(sessionId: string) {
		const turn = this.turns.get(sessionId);
		if (!turn) throw new ApiError(409, "Nada rodando.");
		this.turns.delete(sessionId);
		this.working.get(turn.room)?.delete(turn.agentId);
		this.stopped.push({ room: turn.room, agentId: turn.agentId, sessionId, turnId: turn.turnId, reason: "Parado por você", prompt: turn.prompt });
		this.version += 1;
		this.notify();
		return { ok: true };
	}

	/** The answer to a rooms route, or `undefined` when the path is not one. */
	handle(method: string, url: URL, body: unknown): unknown {
		const path = url.pathname;
		const after = url.searchParams.get("after");
		const unchanged = (scope: string) => (after === this.snapshot(scope) ? { unchanged: true, snapshotId: after } : null);
		if (method === "GET" && path === "/api/v1/mobile/agents") {
			return unchanged("agents") ?? { snapshotId: this.snapshot("agents"), agents: this.agents() };
		}
		if (method === "GET" && path === "/api/v1/mobile/rooms") {
			return unchanged("rooms") ?? { snapshotId: this.snapshot("rooms"), rooms: this.rooms() };
		}
		if (method === "GET" && path === "/api/v1/mobile/room") {
			const now = Date.now();
			this.settle(now);
			const key = this.key(url.searchParams.get("room"));
			const turns = this.roomRead(key, now);
			// While someone works the snapshot moves with the clock, as it does with what streams.
			const beat = turns.live.length > 0 ? `-${Math.floor(now / 500)}` : "";
			const scope = `room-${key}${beat}`;
			const linesId = this.snapshot(`lines-${key}`);
			const held = url.searchParams.get("lines") === linesId;
			return (
				unchanged(scope) ?? {
					snapshotId: this.snapshot(scope),
					linesId,
					...(held ? { linesUnchanged: true } : { lines: this.lines.get(key) ?? [] }),
					...turns,
					approvals:
						this.approvalOpen && key === "loja"
							? [{ sessionId: APPROVAL_SESSION, requestId: "demo-room-p1", agentId: "demo-dev", agentName: "Dev", toolName: "Bash", summary: "yarn test checkout", mayAlwaysAllow: false }]
							: [],
					questions:
						this.questionOpen && key === "site"
							? [{ sessionId: QUESTION_SESSION, requestId: "demo-room-q1", agentId: "demo-rev", agentName: "Revisor", questions: QUESTION }]
							: [],
					working: [...(this.working.get(key) ?? [])],
					cards: this.cards.filter((card) => card.room === key).map(({ room: _room, ...card }) => card),
				}
			);
		}
		if (method === "POST" && path === "/api/v1/mobile/room/reactions") return this.react(body);
		if (method === "POST" && path === "/api/v1/mobile/room/rules") {
			const rule = typeof (body as { body?: unknown })?.body === "string" ? (body as { body: string }).body.trim() : "";
			if (!rule) throw new ApiError(400, "Regra vazia.");
			return { ok: true, noteId: "demo-note" };
		}
		if (method === "POST" && path === "/api/v1/mobile/room/proposals") {
			const id = (body as { id?: unknown })?.id;
			return this.answerCard((card) => (card.proposal as { id?: unknown } | undefined)?.id === id);
		}
		if (method === "POST" && path === "/api/v1/mobile/room/limit") {
			const agentId = (body as { agentId?: unknown })?.agentId;
			return this.answerCard((card) => card.kind === "limit" && card.agentId === agentId);
		}
		if (method === "POST" && path === "/api/v1/mobile/room/messages") return this.send(body);
		if (method === "POST" && path === "/api/v1/mobile/room/handoff") return this.handoff(body);
		if (method === "GET" && path === "/api/v1/mobile/room/steps") return { steps: STEPS, more: 0 };
		const abort = /^\/api\/v1\/sessions\/(demo-live-[^/]+)\/abort$/.exec(path);
		if (method === "POST" && abort) return this.abort(decodeURIComponent(abort[1]!));
		if (
			method === "POST" &&
			(path === `/api/v1/sessions/${APPROVAL_SESSION}/respond-permission` ||
				(path === "/api/v1/mobile/room/approvals/respond" && (body as { sessionId?: unknown })?.sessionId === APPROVAL_SESSION))
		) {
			if (!this.approvalOpen) throw new ApiError(409, "Já foi respondido em outro lugar.");
			this.approvalOpen = false;
			this.version += 1;
			this.notify();
			return { ok: true };
		}
		if (method === "POST" && path === `/api/v1/sessions/${QUESTION_SESSION}/respond-user-input`) {
			if (!this.questionOpen) throw new ApiError(409, "Já foi respondido em outro lugar.");
			this.questionOpen = false;
			const answers = Array.isArray((body as { answers?: unknown })?.answers)
				? ((body as { answers: Array<{ answer?: unknown }> }).answers.map((a) => String(a.answer ?? "")).filter(Boolean))
				: [];
			const list = this.lines.get("site") ?? [];
			list.push({
				key: `demo-site-answer-${list.length}`,
				agentId: "demo-rev",
				sessionId: THREAD,
				speaker: "agent",
				text: `Combinado: ${answers.join(" · ") || "sigo com o padrão"}.`,
				at: new Date().toISOString(),
				receipt: { seconds: 6, tokens: 900 },
			});
			this.lines.set("site", list);
			this.version += 1;
			this.notify();
			return { ok: true };
		}
		return undefined;
	}
}
