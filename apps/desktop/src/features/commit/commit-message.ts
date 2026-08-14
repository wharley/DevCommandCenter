export type CommitMessageChange = {
	path: string;
	status: string;
};

export type WorkspaceCommitMessageSuggestion = {
	subject: string;
	body?: string | null;
	stagedFileCount?: number;
	stagedFingerprint?: string;
	source?: string;
};

const GENERIC_PATH_SEGMENTS = new Set([
	"app",
	"apps",
	"crate",
	"crates",
	"lib",
	"libs",
	"package",
	"packages",
	"src",
	"source",
]);

const GENERIC_FILE_NAMES = new Set([
	"index",
	"lib",
	"main",
	"mod",
]);

const DOCUMENTATION_FILE_PATTERN = /(^|\/)(docs?|readme|changelog|contributing|license)(\/|\.|$)|\.(?:md|mdx|rst|adoc)$/iu;
const TEST_FILE_PATTERN = /(^|\/)(?:__tests__|tests?|specs?)(\/|\.|$)|\.(?:spec|test)\.[^.]+$/iu;
const CI_FILE_PATTERN = /(^|\/)(?:\.github\/workflows|\.gitlab(?:-ci)?|\.circleci|\.buildkite)(\/|\.|$)/iu;
const BUILD_FILE_PATTERN = /(^|\/)(?:package(?:-lock)?\.json|yarn\.lock|pnpm-lock\.yaml|bun\.lockb?|cargo\.(?:toml|lock)|go\.(?:mod|sum)|composer\.(?:json|lock)|pyproject\.toml|poetry\.lock|dockerfile|makefile)$/iu;

function uniqueChanges(changes: readonly CommitMessageChange[]) {
	const byPath = new Map<string, CommitMessageChange>();
	for (const change of changes) {
		const path = change.path.trim().replaceAll("\\", "/");
		if (path) byPath.set(path, { path, status: change.status.trim().toUpperCase() });
	}
	return [...byPath.values()];
}

function commitType(changes: readonly CommitMessageChange[]) {
	const paths = changes.map((change) => change.path);
	if (paths.every((path) => DOCUMENTATION_FILE_PATTERN.test(path))) return "docs";
	if (paths.every((path) => TEST_FILE_PATTERN.test(path))) return "test";
	if (paths.every((path) => CI_FILE_PATTERN.test(path))) return "ci";
	if (paths.every((path) => BUILD_FILE_PATTERN.test(path))) return "build";
	return "chore";
}

function withoutExtension(fileName: string) {
	if (/^(?:readme|changelog|contributing|license)(?:\..+)?$/iu.test(fileName)) {
		return fileName.replace(/\..*$/u, "").toLocaleUpperCase();
	}
	return fileName.replace(/(?:\.d)?\.[^.]+$/u, "");
}

function humanize(value: string) {
	return value
		.replace(/([a-z\d])([A-Z])/gu, "$1 $2")
		.replace(/[-_.]+/gu, " ")
		.replace(/\s+/gu, " ")
		.trim()
		.toLocaleLowerCase();
}

function meaningfulDirectory(path: string) {
	const directories = path.split("/").slice(0, -1);
	return [...directories]
		.reverse()
		.find((segment) => !GENERIC_PATH_SEGMENTS.has(segment.toLocaleLowerCase())) ?? null;
}

function sharedScope(changes: readonly CommitMessageChange[]) {
	const scopes = changes.map((change) => meaningfulDirectory(change.path));
	const first = scopes[0]?.toLocaleLowerCase() ?? null;
	if (!first || scopes.some((scope) => scope?.toLocaleLowerCase() !== first)) return null;
	return first.replace(/[^a-z0-9-]+/gu, "-").replace(/^-+|-+$/gu, "") || null;
}

function nonRedundantScope(type: string, scope: string | null) {
	if (type === "docs" && (scope === "doc" || scope === "docs")) return null;
	if (type === "ci" && scope === "workflows") return null;
	return scope;
}

function changeVerb(changes: readonly CommitMessageChange[]) {
	if (changes.every((change) => change.status.startsWith("A") || change.status === "??")) {
		return "add";
	}
	if (changes.every((change) => change.status.startsWith("D"))) return "remove";
	if (changes.every((change) => change.status.startsWith("R"))) return "rename";
	return "update";
}

function singleChangeSubject(change: CommitMessageChange) {
	const segments = change.path.split("/").filter(Boolean);
	const rawFileName = withoutExtension(segments.at(-1) ?? "");
	const fileName = /^[A-Z]+$/u.test(rawFileName) ? rawFileName : humanize(rawFileName);
	if (fileName && !GENERIC_FILE_NAMES.has(fileName)) return fileName;
	return humanize(meaningfulDirectory(change.path) ?? "project files");
}

function truncateSubject(message: string) {
	if (message.length <= 72) return message;
	return message.slice(0, 72).replace(/\s+\S*$/u, "").trimEnd();
}

/** Sanitizes a subject before Git receives it. */
export function sanitizeWorkspaceCommitSubject(input: string) {
	const firstLine = input
		.replace(/```(?:json|text)?/giu, "")
		.replace(/```/gu, "")
		.trim()
		.split(/\r?\n/u)[0]
		.replace(/^\s*[{[]?\s*["']?(?:subject|message)["']?\s*:\s*/iu, "")
		.replace(/^["']+/u, "")
		.replace(/[}"'\],]+\s*$/u, "")
		.trim();
	return truncateSubject(firstLine) || "chore: update project files";
}

/** Preserves an optional multiline body while removing structured wrappers. */
export function sanitizeWorkspaceCommitBody(input?: string | null) {
	if (!input?.trim()) return null;
	const body = input
		.replace(/^\s*```(?:text|markdown)?\s*$/gimu, "")
		.replace(/^\s*```\s*$/gimu, "")
		.split(/\r?\n/u)
		.filter((line) => !/^\s*["']?(?:subject|message|body)["']?\s*:/iu.test(line))
		.join("\n")
		.trim();
	return body || null;
}

/** Backwards-compatible subject-only helper for deterministic fallback callers. */
export function sanitizeWorkspaceCommitMessage(input: string) {
	return sanitizeWorkspaceCommitSubject(input);
}

/**
 * Creates a truthful offline fallback from the paths that will actually be
 * committed. It deliberately accepts no task/chat title so prompt text can
 * never leak into Git history through this path.
 */
export function deriveWorkspaceCommitMessage(input: readonly CommitMessageChange[]) {
	const changes = uniqueChanges(input);
	if (changes.length === 0) return "chore: update project files";

	const type = commitType(changes);
	const scope = nonRedundantScope(type, sharedScope(changes));
	const prefix = scope ? `${type}(${scope})` : type;
	const verb = changeVerb(changes);
	const subject = changes.length === 1
		? `${verb} ${singleChangeSubject(changes[0])}`
		: scope
			? `${verb} ${humanize(scope)} files`
			: `${verb} project files`;

	return sanitizeWorkspaceCommitSubject(`${prefix}: ${subject}`);
}
