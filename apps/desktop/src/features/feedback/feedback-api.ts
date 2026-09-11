import { invoke } from "@tauri-apps/api/core";

export const FEEDBACK_REPOSITORY = "wharley/DevCommandCenter";
export type FeedbackCategory = "bug" | "improvement" | "other";
export type FeedbackContext = {
	login: string;
	version: string;
	platform: string;
	architecture: string;
};
export type FeedbackInput = {
	requestId: string;
	login: string;
	category: FeedbackCategory;
	title: string;
	description: string;
	steps: string;
	expected: string;
	includeDiagnostics: boolean;
};
export type FeedbackIssue = {
	number: number;
	title: string;
	url: string;
	category: FeedbackCategory;
	state: string;
	stateReason: string | null;
	createdAt: string;
};
export type FeedbackPage = { issues: FeedbackIssue[]; hasNext: boolean };
export const feedbackApi = {
	context: () => invoke<FeedbackContext>("dcc_feedback_context"),
	list: (login: string, page: number) =>
		invoke<FeedbackPage>("dcc_feedback_list", { login, page }),
	create: (input: FeedbackInput) =>
		invoke<FeedbackIssue>("dcc_feedback_create", { input }),
};

export function feedbackStatus(issue: FeedbackIssue) {
	if (issue.state === "open") return "open";
	if (issue.stateReason === "completed") return "completed";
	if (issue.stateReason === "not_planned") return "notPlanned";
	return "closed";
}

export function feedbackError(error: unknown) {
	const message = String(error);
	if (message.includes("FEEDBACK_UNCERTAIN")) return "uncertain";
	if (message.includes("FEEDBACK_ACCOUNT_CHANGED")) return "accountChanged";
	if (
		message.includes("FEEDBACK_AUTH_REQUIRED") ||
		message.includes("FEEDBACK_GITHUB_401")
	)
		return "authRequired";
	if (
		message.includes("FEEDBACK_GITHUB_403") ||
		message.includes("FEEDBACK_GITHUB_429")
	)
		return "forbidden";
	if (message.includes("FEEDBACK_GITHUB_422")) return "rejected";
	return "failed";
}

export type FeedbackDraft = Omit<FeedbackInput, "login"> & {
	pending: FeedbackInput | null;
};
const DRAFT_KEY = "dcc.feedback.draft.v1";
export const emptyFeedbackDraft = (): FeedbackDraft => ({
	requestId: crypto.randomUUID(),
	category: "bug",
	title: "",
	description: "",
	steps: "",
	expected: "",
	includeDiagnostics: true,
	pending: null,
});
export function readFeedbackDraft(): FeedbackDraft {
	try {
		const value = JSON.parse(localStorage.getItem(DRAFT_KEY) ?? "null");
		if (
			value &&
			["bug", "improvement", "other"].includes(value.category) &&
			["requestId", "title", "description", "steps", "expected"].every(
				(key) => typeof value[key] === "string",
			) &&
			typeof value.includeDiagnostics === "boolean"
		)
			return { ...value, pending: value.pending ?? null };
	} catch {
		/* A damaged or unavailable draft must not prevent opening feedback. */
	}
	return emptyFeedbackDraft();
}
export function saveFeedbackDraft(draft: FeedbackDraft) {
	try {
		localStorage.setItem(DRAFT_KEY, JSON.stringify(draft));
		return true;
	} catch {
		return false;
	}
}
const historyKey = (login: string, page: number) =>
	`dcc.feedback.history.v1.${login.toLowerCase()}.${page}`;
export function readFeedbackHistory(
	login: string,
	page: number,
): FeedbackPage | undefined {
	try {
		const value = JSON.parse(
			localStorage.getItem(historyKey(login, page)) ?? "null",
		);
		if (
			value &&
			Array.isArray(value.issues) &&
			typeof value.hasNext === "boolean" &&
			value.issues.every(
				(issue: FeedbackIssue) =>
					Number.isSafeInteger(issue.number) &&
					typeof issue.title === "string" &&
					typeof issue.createdAt === "string" &&
					typeof issue.state === "string",
			)
		)
			return value;
	} catch {
		/* Offline history is best effort. */
	}
	return undefined;
}
export function saveFeedbackHistory(
	login: string,
	page: number,
	data: FeedbackPage,
) {
	try {
		localStorage.setItem(historyKey(login, page), JSON.stringify(data));
	} catch {
		/* Keep the live result. */
	}
}
