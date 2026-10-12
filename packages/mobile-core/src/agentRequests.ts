/**
 * Agent requests outside the permission path, as the desktop serves them
 * (`GET /api/v1/mobile/agent-requests`, `event: agent-requests`): open the
 * in-app Browser at a URL, or let the agent drive apps on the Mac.
 *
 * The first answer wins on the desktop; a late one gets 409 with who
 * answered. The phone may approve both kinds, with what the Mac's screen
 * still has to do said plainly on the card.
 */

export type AgentRequestApp = { bundleId: string; name: string };

type Base = {
	requestId: string;
	sessionId: string;
	workspaceId: string;
	providerId: string;
	reason: string;
	expiresAtMs: number;
};

export type AgentRequest =
	| (Base & { kind: "browserOpen"; url: string })
	| (Base & { kind: "computerControl"; apps: AgentRequestApp[]; blocker: string | null });

export type AgentRequestOutcome = {
	requestId: string;
	decision: "allowed" | "denied";
	by: "desktop" | "phone";
	at: string;
};

export const BROWSER_HANDOFF_NOTICE =
	"Login, MFA, CAPTCHA ou pagamento nessa página só podem ser feitos na tela do Mac.";
export const COMPUTER_CONTROL_WARNING =
	"Ao permitir, o agente vai controlar a tela do Mac sem ninguém na frente dele, nos apps que você marcar.";

function text(value: unknown): string | null {
	return typeof value === "string" && value.length > 0 ? value : null;
}

function parseOne(raw: unknown): AgentRequest | null {
	if (!raw || typeof raw !== "object") return null;
	const value = raw as Record<string, unknown>;
	const requestId = text(value.requestId);
	const sessionId = text(value.sessionId);
	if (!requestId || !sessionId) return null;
	const base: Base = {
		requestId,
		sessionId,
		workspaceId: text(value.workspaceId) ?? "",
		providerId: text(value.providerId) ?? "",
		reason: text(value.reason) ?? "",
		expiresAtMs: typeof value.expiresAtMs === "number" ? value.expiresAtMs : 0,
	};
	if (value.kind === "browserOpen") {
		const url = text(value.url);
		return url ? { ...base, kind: "browserOpen", url } : null;
	}
	if (value.kind === "computerControl") {
		const apps = Array.isArray(value.apps)
			? value.apps.flatMap((app): AgentRequestApp[] => {
					const entry = (app ?? {}) as Record<string, unknown>;
					const bundleId = text(entry.bundleId);
					return bundleId ? [{ bundleId, name: text(entry.name) ?? bundleId }] : [];
				})
			: [];
		return { ...base, kind: "computerControl", apps, blocker: text(value.blocker) };
	}
	return null;
}

/** `{ requests: [...] }` from the list route or the stream; unknown kinds dropped. */
export function parseAgentRequests(data: unknown): AgentRequest[] {
	const list = (data as { requests?: unknown } | null)?.requests;
	if (!Array.isArray(list)) return [];
	return list.flatMap((raw) => {
		const request = parseOne(raw);
		return request ? [request] : [];
	});
}

export function agentRequestTitle(request: AgentRequest): string {
	return request.kind === "browserOpen" ? "Abrir o navegador" : "Controlar o computador";
}

/** Seconds left to answer; 0 once the agent gave up. */
export function secondsLeft(request: AgentRequest, now = Date.now()): number {
	return Math.max(0, Math.ceil((request.expiresAtMs - now) / 1000));
}

export function openAgentRequests(requests: AgentRequest[], now = Date.now()): AgentRequest[] {
	return requests.filter((request) => request.expiresAtMs > now);
}

export function agentRequestsForSession(requests: AgentRequest[], sessionId: string, now = Date.now()): AgentRequest[] {
	return openAgentRequests(requests, now).filter((request) => request.sessionId === sessionId);
}

/** Why "Permitir" is off, or null when it can be pressed. */
export function allowBlocker(request: AgentRequest, pickedApps: readonly string[], now = Date.now()): string | null {
	if (secondsLeft(request, now) === 0) return "O pedido expirou.";
	if (request.kind === "browserOpen") return null;
	if (request.blocker) return request.blocker;
	if (pickedApps.length === 0) return "Marque pelo menos um app.";
	return null;
}

/** The body of a 409: who answered first, in words. */
export function outcomeText(outcome: AgentRequestOutcome | null | undefined): string {
	if (!outcome) return "Esse pedido já tinha sido respondido em outro lugar.";
	const verb = outcome.decision === "allowed" ? "aprovado" : "negado";
	return outcome.by === "phone" ? `Esse pedido já foi ${verb} pelo celular.` : `Esse pedido já foi ${verb} no computador.`;
}

/** The outcome inside an `ApiError` body (`{ error: { outcome } }`). */
export function conflictOutcome(body: unknown): AgentRequestOutcome | null {
	const outcome = (body as { error?: { outcome?: unknown } } | null)?.error?.outcome;
	if (!outcome || typeof outcome !== "object") return null;
	const value = outcome as Record<string, unknown>;
	if ((value.decision === "allowed" || value.decision === "denied") && (value.by === "phone" || value.by === "desktop")) {
		return {
			requestId: text(value.requestId) ?? "",
			decision: value.decision,
			by: value.by,
			at: text(value.at) ?? "",
		};
	}
	return null;
}
