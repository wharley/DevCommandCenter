/**
 * The resident agents as the desktop serves them to the phone
 * (`GET /api/v1/mobile/agents`): each one that talks, with its mascot, how
 * it is, its own conversation to open and its latest news. Like the
 * attention snapshot, a phone that sends back the `snapshotId` it holds gets
 * `{ unchanged, snapshotId }` while nothing changed.
 *
 * The mascot is the desktop's own drawing (`apps/native/src/agent/avatar.rs`
 * and `mood.rs`), element for element, so the octopus looks the same.
 */

/** Projects of the agents' own conversations; they are not tasks. */
export const AGENT_HOME_PREFIX = "dcc-agent-";

export function isAgentHome(projectId: string | null | undefined): boolean {
	return !!projectId && projectId.startsWith(AGENT_HOME_PREFIX) && projectId.length > AGENT_HOME_PREFIX.length;
}

export type ResidentAgentState = "needsYou" | "working" | "watching" | "asleep";

export type ResidentAgentAvatar = { color: string; arms: number; eyes: string };

export type ResidentAgent = {
	id: string;
	name: string;
	preset: string | null;
	avatar: ResidentAgentAvatar;
	state: ResidentAgentState;
	/** Its own conversation, once it has one (started on the desktop). */
	conversationSessionId: string | null;
	/** Its latest finished turn, anywhere. */
	last: { sessionId: string; title: string | null; at: string; inConversation: boolean } | null;
};

export type ResidentAgentsSnapshot =
	| { unchanged: true; snapshotId: string }
	| { unchanged: false; snapshotId: string | null; agents: ResidentAgent[] };

const STATES: ResidentAgentState[] = ["needsYou", "working", "watching", "asleep"];

function text(value: unknown): string | null {
	return typeof value === "string" && value.length > 0 ? value : null;
}

function parseAgent(raw: unknown): ResidentAgent | null {
	if (!raw || typeof raw !== "object") return null;
	const value = raw as Record<string, unknown>;
	const id = text(value.id);
	const name = text(value.name);
	if (!id || !name) return null;
	const avatar = (value.avatar ?? {}) as Record<string, unknown>;
	const state = STATES.includes(value.state as ResidentAgentState) ? (value.state as ResidentAgentState) : "asleep";
	const last = value.last as Record<string, unknown> | null | undefined;
	const lastSession = last ? text(last.sessionId) : null;
	const lastAt = last ? text(last.at) : null;
	return {
		id,
		name,
		preset: text(value.preset),
		avatar: {
			color: text(avatar.color) ?? "blue",
			arms: typeof avatar.arms === "number" ? avatar.arms : 3,
			eyes: text(avatar.eyes) ?? "round",
		},
		state,
		conversationSessionId: text(value.conversationSessionId),
		last:
			last && lastSession && lastAt
				? { sessionId: lastSession, title: text(last.title), at: lastAt, inConversation: last.inConversation === true }
				: null,
	};
}

/** The route's answer; `unchanged` only counts when it answers `after`. */
export function parseResidentAgents(data: unknown, after: string | null): ResidentAgentsSnapshot | null {
	if (!data || typeof data !== "object") return null;
	const value = data as Record<string, unknown>;
	const snapshotId = text(value.snapshotId);
	if (value.unchanged === true) {
		return snapshotId && snapshotId === after ? { unchanged: true, snapshotId } : null;
	}
	if (!Array.isArray(value.agents)) return null;
	return {
		unchanged: false,
		snapshotId,
		agents: value.agents.flatMap((raw) => {
			const agent = parseAgent(raw);
			return agent ? [agent] : [];
		}),
	};
}

/** `agents.mood.*` / `agents.state.*` on the desktop. */
export function residentStateLabel(state: ResidentAgentState): string {
	switch (state) {
		case "needsYou":
			return "Precisa de você";
		case "working":
			return "Trabalhando";
		case "watching":
			return "De olho";
		case "asleep":
			return "Dormindo";
	}
}

/** "há 5 min" — the latest news, short. */
export function relativeTime(at: string, now: number): string {
	const then = Date.parse(at);
	if (Number.isNaN(then)) return "";
	const minutes = Math.max(0, Math.round((now - then) / 60_000));
	if (minutes < 1) return "agora";
	if (minutes < 60) return `há ${minutes} min`;
	const hours = Math.round(minutes / 60);
	if (hours < 24) return `há ${hours} h`;
	const days = Math.round(hours / 24);
	return days === 1 ? "ontem" : `há ${days} dias`;
}

/** The card's second line: what it last did, or how to start talking. */
export function residentNews(agent: ResidentAgent, now: number): string {
	if (agent.last) {
		const when = relativeTime(agent.last.at, now);
		const what = agent.last.inConversation ? "Respondeu na conversa" : `Terminou ${agent.last.title ?? "uma tarefa"}`;
		return when ? `${what} · ${when}` : what;
	}
	return agent.conversationSessionId ? "Conversa aberta" : "Comece a conversa no Mac";
}

// ---------- the octopus, as the desktop draws it ----------

const INK = "#0b1220";

function color(name: string): string {
	switch (name) {
		case "cyan":
			return "#20BEFF";
		case "violet":
			return "#8B5CF6";
		case "amber":
			return "#F2A93B";
		case "green":
			return "#34C28A";
		case "pink":
			return "#F472B6";
		default:
			return "#356EFF";
	}
}

function focusEyes(): string {
	return `<ellipse cx="39" cy="46" rx="7.5" ry="8.5" fill="#fff"/><ellipse cx="61" cy="46" rx="7.5" ry="8.5" fill="#fff"/><circle cx="39" cy="50" r="3.8" fill="${INK}"/><circle cx="61" cy="50" r="3.8" fill="${INK}"/>`;
}

function looking(dx: number, dy: number): string {
	return `<ellipse cx="39" cy="46" rx="7.5" ry="8.5" fill="#fff"/><ellipse cx="61" cy="46" rx="7.5" ry="8.5" fill="#fff"/><circle cx="${39 + dx}" cy="${46 + dy}" r="3.8" fill="${INK}"/><circle cx="${61 + dx}" cy="${46 + dy}" r="3.8" fill="${INK}"/>`;
}

function roundEyes(): string {
	return looking(1.5, 1);
}

const SLEEPING_EYES = `<path d="M32 45q7 6 14 0M54 45q7 6 14 0" fill="none" stroke="${INK}" stroke-width="4.5" stroke-linecap="round"/>`;
const ZZ = `<path d="M78 14h8l-8 9h8M88 4h5l-5 6h5" fill="none" stroke="#94a3b8" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"/>`;
const WAIT_BADGE = `<circle cx="82" cy="80" r="15" fill="#F2A93B"/><path d="M82 72v9" stroke="${INK}" stroke-width="4" stroke-linecap="round"/><circle cx="82" cy="87" r="2.3" fill="${INK}"/>`;

/** The mascot of `avatar` as the agent is, as an SVG document. */
export function residentMascotSvg(avatar: ResidentAgentAvatar, state: ResidentAgentState): string {
	const fill = color(avatar.color);
	const arms = Math.min(5, Math.max(3, Math.round(avatar.arms)));
	const gap = 3;
	const width = (68 - gap * (arms - 1)) / arms;
	let body = "";
	if (state === "needsYou") {
		body += `<rect x="78" y="14" width="13" height="40" rx="6.5" fill="${fill}" transform="rotate(24 84 52)"/>`;
	}
	for (let index = 0; index < arms; index++) {
		const height = index % 2 === 0 ? 26 : 20;
		body += `<rect x="${16 + index * (width + gap)}" y="56" width="${width}" height="${height}" rx="${width / 2}" fill="${fill}"/>`;
	}
	body += `<path d="M16 60V52C16 29 32 14 50 14s34 15 34 38v8z" fill="${fill}"/>`;
	body += `<ellipse cx="36" cy="27" rx="9" ry="5" fill="#fff" opacity="0.22" transform="rotate(-28 36 27)"/>`;
	body +=
		state === "asleep" ? SLEEPING_EYES : state === "watching" ? looking(3, 0) : state === "working" ? focusEyes() : roundEyes();
	const opacity = state === "asleep" ? 0.6 : 1;
	const badge = state === "needsYou" ? WAIT_BADGE : "";
	const extra = state === "asleep" ? ZZ : "";
	return `<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100" viewBox="0 0 100 100"><g opacity="${opacity}">${body}</g>${badge}${extra}</svg>`;
}
