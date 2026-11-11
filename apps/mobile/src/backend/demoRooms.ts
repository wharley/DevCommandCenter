/**
 * The rooms of the demo desktop (`demo.ts`): three resident agents, Geral
 * and two project rooms with a few lines, one approval and one question
 * waiting in a room and an agent that answers what the person writes. Same paths and shapes
 * as the real desktop (`/api/v1/mobile/rooms`, `/mobile/room`, `/mobile/agents`).
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
};

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
		line("loja", "demo-dev", "agent", "Comecei pela camada de pagamento. Abri a tarefa **checkout-v2** para isso; o gateway antigo fica atrás de uma flag.", 12, { seconds: 95, tokens: 14300 });
		const opened = this.lines.get("loja")!.at(-1)!;
		opened.steps = { seconds: 95, tools: 6, failures: 0, counts: { read: 4, command: 1, edit: 1 } };
		opened.task = { delegationId: "demo-d1", workspaceId: "demo-w1", sessionId: "demo-s1", title: "checkout-v2", state: "working", files: 3, additions: 78, deletions: 14 };
		line("site", "demo-rev", "agent", "O guia de onboarding tem um check de lint falhando. Sugiro corrigir antes de mesclar.", 60 * 5, { seconds: 22, tokens: 3100 });
		this.working.set("loja", new Set(["demo-dev"]));
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
		const input = (body ?? {}) as { text?: unknown; room?: unknown };
		const text = typeof input.text === "string" ? input.text.trim() : "";
		if (!text) throw new ApiError(400, "Mensagem vazia.");
		const key = this.key(typeof input.room === "string" && input.room ? input.room : null);
		const called = AGENTS.find((agent) => text.toLowerCase().includes(`@${agent.name.toLowerCase()}`));
		const last = [...(this.lines.get(key) ?? [])].reverse().find((line) => line.speaker === "agent");
		const agent = called ?? AGENTS.find((a) => a.id === last?.agentId) ?? AGENTS[1]!;
		const push = (speaker: Line["speaker"], body: string, receipt: Line["receipt"] = null) => {
			const list = this.lines.get(key) ?? [];
			this.counter += 1;
			list.push({ key: `demo-${key}-new-${this.counter}`, agentId: agent.id, sessionId: THREAD, speaker, text: body, at: new Date().toISOString(), receipt });
			this.lines.set(key, list);
			this.version += 1;
		};
		push("person", text);
		const set = this.working.get(key) ?? new Set<string>();
		set.add(agent.id);
		this.working.set(key, set);
		this.later(() => {
			push("agent", "Anotado. No modo demonstração eu só respondo isto; com o DCC no computador, a resposta vem do agente de verdade.", { seconds: 4, tokens: 620 });
			set.delete(agent.id);
			this.notify();
		}, 2200);
		this.notify();
		return { agentId: agent.id, agentName: agent.name, reason: called ? "mention" : last ? "continues" : "default", sessionId: THREAD, room: key === "geral" ? null : key };
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
			const key = this.key(url.searchParams.get("room"));
			return (
				unchanged(`room-${key}`) ?? {
					snapshotId: this.snapshot(`room-${key}`),
					lines: this.lines.get(key) ?? [],
					approvals:
						this.approvalOpen && key === "loja"
							? [{ sessionId: APPROVAL_SESSION, requestId: "demo-room-p1", agentId: "demo-dev", agentName: "Dev", toolName: "Bash", summary: "yarn test checkout" }]
							: [],
					questions:
						this.questionOpen && key === "site"
							? [{ sessionId: QUESTION_SESSION, requestId: "demo-room-q1", agentId: "demo-rev", agentName: "Revisor", questions: QUESTION }]
							: [],
					working: [...(this.working.get(key) ?? [])],
				}
			);
		}
		if (method === "POST" && path === "/api/v1/mobile/room/messages") return this.send(body);
		if (method === "POST" && path === `/api/v1/sessions/${APPROVAL_SESSION}/respond-permission`) {
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
