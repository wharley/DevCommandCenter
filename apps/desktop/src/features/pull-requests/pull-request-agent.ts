import type {
	PullRequestHubDetailOutput,
	PullRequestHubDraftComment,
	PullRequestHubInlineComment,
	PullRequestHubItem,
} from "@dcc/contracts";
import { parseUnifiedDiff, type UnifiedDiffLine } from "./unified-diff";

export type PullRequestAgentReview = {
	summary: string;
	comments: PullRequestHubDraftComment[];
};

function fencedJson(value: string) {
	const fenced = value.match(/```(?:json)?\s*([\s\S]*?)```/i)?.[1];
	return (fenced ?? value).trim();
}

function bounded(value: string, max: number) {
	return value.length <= max ? value : `${value.slice(0, max)}\n…[truncated]`;
}

function pullRequestContext(item: PullRequestHubItem, detail: PullRequestHubDetailOutput) {
	const files = detail.files
		.map((file) => `### ${file.path}\n${file.patch ?? "[patch unavailable]"}`)
		.join("\n\n");
	return bounded(
		[
			`PR: ${item.repositoryName}#${item.number} — ${item.title}`,
			`Author: ${item.author?.login ?? "unknown"}`,
			`Branches: ${item.headBranch} -> ${item.baseBranch}`,
			`Description:\n${detail.body ?? item.body ?? "[none]"}`,
			`Changed files:\n${files}`,
		].join("\n\n"),
		95_000,
	);
}

export function buildReviewAgentPrompt(
	item: PullRequestHubItem,
	detail: PullRequestHubDetailOutput,
	instruction: string,
) {
	return `${pullRequestContext(item, detail)}

Review this pull request for correctness, regressions, security, data loss, concurrency, and missing tests. Focus only on actionable findings introduced by the diff. ${instruction.trim() || ""}

Return JSON only with this exact shape:
{"summary":"editable review summary","comments":[{"path":"exact changed file path","line":123,"side":"right","body":"concise actionable finding"}]}

Use only lines visible in the supplied patches. If there are no findings, return an empty comments array. Do not publish or modify anything.`;
}

export function buildInlineAgentPrompt(
	item: PullRequestHubItem,
	path: string,
	row: UnifiedDiffLine,
	patch: string | null,
	instruction: string,
) {
	return `${bounded(`PR: ${item.repositoryName}#${item.number} — ${item.title}\nFile: ${path}\nSelected ${row.reviewSide} line ${row.reviewLine}: ${row.content}\nPatch:\n${patch ?? "[unavailable]"}`, 95_000)}

Draft one precise inline review comment for the selected line. ${instruction.trim() || ""}
Return JSON only: {"reply":"editable comment text"}. Do not publish or modify anything.`;
}

export function buildThreadReplyAgentPrompt(
	item: PullRequestHubItem,
	comment: PullRequestHubInlineComment,
	patch: string | null,
	instruction: string,
) {
	return `${bounded(`PR: ${item.repositoryName}#${item.number} — ${item.title}\nThread: ${comment.path}:${comment.line ?? "unknown"}\nComment by @${comment.author?.login ?? "unknown"}:\n${comment.body}\nPatch:\n${patch ?? "[unavailable]"}`, 95_000)}

Draft a helpful direct reply to this review thread. ${instruction.trim() || ""}
Return JSON only: {"reply":"editable reply text"}. Do not publish or resolve anything.`;
}

export function parseAgentReply(response: string) {
	const parsed = JSON.parse(fencedJson(response)) as { reply?: unknown };
	if (typeof parsed.reply !== "string" || !parsed.reply.trim()) {
		throw new Error("The agent did not return a usable draft.");
	}
	return parsed.reply.trim();
}

export function parseAgentReview(
	response: string,
	detail: PullRequestHubDetailOutput,
): PullRequestAgentReview {
	const parsed = JSON.parse(fencedJson(response)) as {
		summary?: unknown;
		comments?: unknown;
	};
	const reviewable = new Set<string>();
	for (const file of detail.files) {
		for (const row of parseUnifiedDiff(file.patch)) {
			if (row.reviewLine != null && row.reviewSide) {
				reviewable.add(`${file.path}:${row.reviewSide}:${row.reviewLine}`);
			}
		}
	}
	const comments = Array.isArray(parsed.comments)
		? parsed.comments.flatMap((value): PullRequestHubDraftComment[] => {
				if (!value || typeof value !== "object") return [];
				const candidate = value as Record<string, unknown>;
				const path = typeof candidate.path === "string" ? candidate.path : "";
				const line = typeof candidate.line === "number" ? candidate.line : 0;
				const side = candidate.side === "left" ? "left" : "right";
				const body = typeof candidate.body === "string" ? candidate.body.trim() : "";
				if (!body || !reviewable.has(`${path}:${side}:${line}`)) return [];
				return [{ path, line, side, body }];
			})
		: [];
	return {
		summary: typeof parsed.summary === "string" ? parsed.summary.trim() : "",
		comments,
	};
}
