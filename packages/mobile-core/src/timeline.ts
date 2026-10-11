/**
 * Presentation rules for a thread's folded messages in the phone app.
 *
 * The raw stream interleaves conversation (user/assistant) with machinery
 * (reasoning, tool calls, resolved permissions). Rendering the machinery at
 * the same visual level as the conversation is what made the thread feel
 * chaotic — so contiguous runs of machinery fold into one collapsible
 * "trace" capsule, and the chat reads user → trace → answer again.
 * Unresolved permissions stay standalone: they need the user's thumb.
 */
import { t, toolStepParts } from "./labels.ts";
import type { ChatMessage } from "./threadEvents.ts";

export type TraceStep = Extract<
	ChatMessage,
	{ kind: "reasoning" } | { kind: "tool" } | { kind: "permission" }
>;

export type RenderItem =
	| { type: "single"; key: string; message: ChatMessage }
	| { type: "trace"; key: string; steps: TraceStep[] };

export function messageKey(msg: ChatMessage, fallbackIndex: number): string {
	switch (msg.kind) {
		case "user":
		case "assistant":
			return `${msg.kind}-${msg.turnId}`;
		case "reasoning":
			return `reasoning-${msg.id}`;
		case "tool":
			return `tool-${msg.toolCallId}`;
		case "permission":
			return `perm-${msg.requestId}`;
		case "system":
			return `sys-${msg.id}`;
		default:
			return `evt-${fallbackIndex}`;
	}
}

export function groupMessages(messages: ChatMessage[]): RenderItem[] {
	const items: RenderItem[] = [];
	let run: TraceStep[] = [];
	const flush = () => {
		if (run.length > 0) {
			const first = run[0]!;
			items.push({
				type: "trace",
				key: `trace-${messageKey(first, 0)}`,
				steps: run,
			});
			run = [];
		}
	};
	for (let i = 0; i < messages.length; i++) {
		const msg = messages[i];
		if (!msg) continue;
		const isStep =
			msg.kind === "reasoning" ||
			msg.kind === "tool" ||
			(msg.kind === "permission" && msg.resolved);
		if (isStep) {
			run.push(msg as TraceStep);
			continue;
		}
		flush();
		items.push({ type: "single", key: messageKey(msg, i), message: msg });
	}
	flush();
	return items;
}

/**
 * Items that open an agent's reply — the first non-user item after each
 * prompt. The thread puts the agent's robot and model label on them, like
 * the desktop's assistant header.
 */
export function agentTurnHeads(items: RenderItem[]): string[] {
	const heads: string[] = [];
	let open = true;
	for (const item of items) {
		const kind = item.type === "single" ? item.message.kind : "trace";
		if (kind === "user") {
			open = true;
			continue;
		}
		if (kind === "system") continue;
		if (open) {
			heads.push(item.key);
			open = false;
		}
	}
	return heads;
}

export function stepIsLive(step: TraceStep): boolean {
	if (step.kind === "reasoning") return !step.completed;
	if (step.kind === "tool") return step.status === "running";
	return false;
}

/**
 * A step as the desktop's activity rows read it: "Pensando"/"Pensou", or a
 * tool's verb ("Rodou", "Editando") with what it acted on. The provider's own
 * names (`Bash`, "Thinking") never reach the screen.
 */
export function stepParts(step: TraceStep): { verb: string; target: string | null } {
	if (step.kind === "reasoning") {
		return {
			verb: t(step.completed ? "conversation.activity.turn.thought" : "conversation.reasoning.label"),
			target: null,
		};
	}
	if (step.kind === "tool") return toolStepParts(step.toolName, step.input, step.status === "running");
	return { verb: firstLine(step.question) || "Pedido de permissão", target: null };
}

export function stepLabel(step: TraceStep): string {
	const { verb, target } = stepParts(step);
	return target ? `${verb} ${target}` : verb;
}

/** The trace capsule's title: what the agent is doing now, or did. */
export function traceTitle(steps: TraceStep[]): string {
	const live = steps.some(stepIsLive);
	return live ? t("conversation.activity.turn.working") : t("conversation.activity.turn.workedNoDuration");
}

export function firstLine(text: string): string {
	return text.split("\n", 1)[0]?.trim() ?? "";
}

/**
 * Running = anything in flight: a streaming answer, a live tool call or
 * reasoning block, or a prompt that hasn't produced output yet. Walking
 * backwards means the freshest signal wins.
 */
export function isThreadRunning(messages: ChatMessage[]): boolean {
	for (let i = messages.length - 1; i >= 0; i--) {
		const m = messages[i];
		if (!m) continue;
		if (m.kind === "assistant") return !m.completed && !m.aborted;
		if (m.kind === "tool") {
			if (m.status === "running") return true;
			continue;
		}
		if (m.kind === "reasoning") {
			if (!m.completed) return true;
			continue;
		}
		if (m.kind === "user") return true;
		if (m.kind === "system") return false;
	}
	return false;
}

/** The first prompt, trimmed to a header-sized line. */
export function threadTitle(messages: ChatMessage[], max = 52): string | null {
	const firstUser = messages.find(
		(m) => m.kind === "user" && m.text.trim().length > 0,
	);
	if (!firstUser || firstUser.kind !== "user") return null;
	const t = firstUser.text.trim().replace(/\s+/g, " ");
	return t.length > max ? `${t.slice(0, max - 1)}…` : t;
}

/**
 * The level the session's latest turn ran with — what a continuation keeps
 * unless the person picks another (the desktop API inherits it the same way).
 */
export function threadApprovalPolicy(messages: ChatMessage[]): string | null {
	for (let i = messages.length - 1; i >= 0; i--) {
		const m = messages[i];
		if (m?.kind === "user" && m.approvalPolicy) return m.approvalPolicy;
	}
	return null;
}
