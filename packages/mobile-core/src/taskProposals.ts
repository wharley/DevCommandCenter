/**
 * Tasks an agent proposed with `dcc_propose_task`, as the desktop serves them
 * (`GET /api/v1/mobile/task-proposals`). Nothing runs until someone starts
 * one; it then becomes a conversation of its own on the Mac and the agent
 * hears back each time it ends a turn. Starting or dismissing answers it on
 * the desktop too; a late answer gets 409.
 */

export type TaskProposal = {
	id: string;
	parentSessionId: string;
	workspaceId: string;
	title: string;
	brief: string;
	provider: string;
	model: string | null;
	/** Changes files (read-only otherwise). */
	edits: boolean;
	/** Its edits land straight in the parent's checkout; no worktree. */
	sharedCheckout: boolean;
	createdAt: string;
};

export const TASK_PROPOSAL_HINT =
	"Nada roda até você começar. Vira uma conversa própria no Mac; o agente recebe um resumo a cada rodada.";

function text(value: unknown): string | null {
	return typeof value === "string" && value.length > 0 ? value : null;
}

function parseOne(raw: unknown): TaskProposal | null {
	if (!raw || typeof raw !== "object") return null;
	const value = raw as Record<string, unknown>;
	const id = text(value.id);
	const parentSessionId = text(value.parentSessionId);
	if (!id || !parentSessionId) return null;
	return {
		id,
		parentSessionId,
		workspaceId: text(value.workspaceId) ?? "",
		title: text(value.title) ?? "Tarefa proposta",
		brief: text(value.brief) ?? "",
		provider: text(value.provider) ?? "",
		model: text(value.model),
		edits: value.edits === true,
		sharedCheckout: value.sharedCheckout === true,
		createdAt: text(value.createdAt) ?? "",
	};
}

/** `{ proposals: [...] }` from the list route; malformed entries dropped. */
export function parseTaskProposals(data: unknown): TaskProposal[] {
	const list = (data as { proposals?: unknown } | null)?.proposals;
	if (!Array.isArray(list)) return [];
	return list.flatMap((raw) => {
		const proposal = parseOne(raw);
		return proposal ? [proposal] : [];
	});
}

export function proposalsForSession(proposals: TaskProposal[], sessionId: string): TaskProposal[] {
	return proposals.filter((proposal) => proposal.parentSessionId === sessionId);
}

/** "Codex · worktree próprio": who runs it and where it may write. */
export function proposalMeta(proposal: TaskProposal): string {
	const agent = proposal.model ?? proposal.provider;
	const scope = !proposal.edits ? "só leitura" : proposal.sharedCheckout ? "direto no checkout" : "worktree próprio";
	return agent ? `${agent} · ${scope}` : scope;
}

/**
 * A `core-event` of the stream that may add or settle a proposal: every
 * delegation event (`{ sessionDelegationRequested: {...} }`, …).
 */
export function isDelegationStreamEvent(data: unknown): boolean {
	let payload = data;
	if (typeof payload === "string") {
		try {
			payload = JSON.parse(payload);
		} catch {
			return false;
		}
	}
	if (!payload || typeof payload !== "object") return false;
	return Object.keys(payload).some((key) => key.startsWith("sessionDelegation"));
}
