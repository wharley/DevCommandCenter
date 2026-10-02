import { fenceLanguageForPath } from "@/components/ai/code-presentation";
import type { DiffMachineAnnotation } from "@/features/editor/diff-types";

/** One reviewer finding as it travels to the author agent. */
export type FindingForAuthor = {
	path: string;
	startLine: number;
	endLine: number;
	title: string;
	/** The reviewer's full explanation: what is wrong, why, what to change. */
	detail: string;
	/** The lines it points at, when the caller has the file at hand. */
	snippet: string;
};

/** Findings the person wants the task's author agent to fix. */
export type FindingToAuthorCommand = {
	workspaceId: string;
	findings: FindingForAuthor[];
};

const FINDING_TO_AUTHOR_EVENT = "dcc:agent-finding-to-author";

/** The lines a finding points at, as they are in the file now. */
export function findingSnippet(text: string, startLine: number, endLine: number): string {
	return text
		.split(/\r?\n/)
		.slice(Math.max(0, startLine - 1), Math.max(startLine, endLine))
		.join("\n");
}

/** Hands findings to the task panel, which drafts a fix request in the author's composer. */
export function dispatchFindingsToAuthor(
	workspaceId: string | null | undefined,
	findings: FindingForAuthor[],
): void {
	if (!workspaceId || findings.length === 0) {
		return;
	}
	window.dispatchEvent(
		new CustomEvent<FindingToAuthorCommand>(FINDING_TO_AUTHOR_EVENT, {
			detail: { workspaceId, findings },
		}),
	);
}

/** A clicked reviewer marking on a diff. Other markings are ignored. */
export function dispatchFindingToAuthor(input: {
	workspaceId: string | null | undefined;
	path: string;
	annotation: DiffMachineAnnotation;
	/** The whole current file, or the marked lines when only a patch is at hand. */
	modifiedText?: string;
	snippet?: string;
}): void {
	const { workspaceId, path, annotation, modifiedText } = input;
	if (annotation.source !== "agent-review") {
		return;
	}
	dispatchFindingsToAuthor(workspaceId, [
		{
			path,
			startLine: annotation.startLine,
			endLine: annotation.endLine,
			title: annotation.title,
			detail: annotation.detail ?? "",
			snippet:
				input.snippet ??
				findingSnippet(modifiedText ?? "", annotation.startLine, annotation.endLine),
		},
	]);
}

export function subscribeFindingToAuthor(
	listener: (command: FindingToAuthorCommand) => void,
): () => void {
	const handler = (event: Event) => listener((event as CustomEvent<FindingToAuthorCommand>).detail);
	window.addEventListener(FINDING_TO_AUTHOR_EVENT, handler);
	return () => window.removeEventListener(FINDING_TO_AUTHOR_EVENT, handler);
}

type SessionLike = {
	session: { id: string };
	lastTurnStartedAt?: string | null;
};

/**
 * The session that wrote the changes: the plain (non-agent) session of the
 * task the person used most recently.
 */
export function authorSessionId(
	sessions: SessionLike[],
	isAgentSession: (sessionId: string) => boolean,
): string | null {
	const plain = sessions.filter((summary) => !isAgentSession(summary.session.id));
	const latest = plain.reduce<SessionLike | null>(
		(best, summary) =>
			!best || (summary.lastTurnStartedAt ?? "") > (best.lastTurnStartedAt ?? "") ? summary : best,
		null,
	);
	return latest?.session.id ?? null;
}

export type FixRequestLabels = {
	/** "Fix this Reviewer finding" */
	one: string;
	/** "Fix these N Reviewer findings", already counted. */
	many: string;
	/** ("line 3") or ("lines 3–13") */
	location: (startLine: number, endLine: number) => string;
};

function findingBody(finding: FindingForAuthor, labels: FixRequestLabels, indent: string): string[] {
	const lines = [`\`${finding.path}\` (${labels.location(finding.startLine, finding.endLine)}): ${finding.title}`];
	if (finding.detail && finding.detail !== finding.title) {
		lines.push(`${indent}${finding.detail}`);
	}
	return lines;
}

/**
 * The draft the author agent receives. It carries the reviewer's full
 * explanation: a one-sentence title alone reads as "already handled" when
 * the code next to it looks reasonable.
 */
export function buildFixRequest(findings: FindingForAuthor[], labels: FixRequestLabels): string {
	if (findings.length === 1) {
		const finding = findings[0]!;
		const parts = [`${labels.one}:`, "", ...findingBody(finding, labels, "")];
		if (finding.snippet.trim()) {
			// The fence names the language and the first line, so the snippet reads
			// as the file does: highlighted, with its real line numbers.
			const fence = `\`\`\`${fenceLanguageForPath(finding.path)} startLine=${finding.startLine}`;
			parts.push("", fence, finding.snippet, "```");
		}
		return parts.join("\n");
	}
	return [
		`${labels.many}:`,
		"",
		...findings.flatMap((finding, index) => {
			const [head, ...rest] = findingBody(finding, labels, "   ");
			return [`${index + 1}. ${head}`, ...rest];
		}),
	].join("\n");
}
