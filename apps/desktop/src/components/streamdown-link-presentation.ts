import type { WorkspaceFileReference } from "./workspace-file-reference";

export type StreamdownLinkKind =
	| "default"
	| "external"
	| "file"
	| "workspace-file";

const CODE_FILE_EXTENSION =
	/\.(?:astro|bash|c|cc|cpp|cs|css|cxx|go|graphql|gql|h|hpp|html|java|js|jsx|json|kt|kts|less|lua|m|mdx|mts|php|py|r|rb|rs|sass|scala|scss|sh|sql|svelte|swift|toml|ts|tsx|vue|xml|yaml|yml|zsh)$/i;

function isExternalWebUrl(href: string | undefined) {
	return /^https?:\/\//i.test(href?.trim() ?? "");
}

export function isLocalFileHref(href: string | undefined) {
	const value = href?.trim() ?? "";
	return value.startsWith("file://") || /^(?:[a-zA-Z]:[\\/]|\/)/.test(value);
}

export function getStreamdownLinkKind(
	href: string | undefined,
	workspaceReference: WorkspaceFileReference | null,
): StreamdownLinkKind {
	if (workspaceReference) {
		return "workspace-file";
	}
	if (isLocalFileHref(href)) {
		return "file";
	}
	if (isExternalWebUrl(href)) {
		return "external";
	}
	return "default";
}

export function isCodeFileReference(path: string) {
	const filePath = path.split(/[?#]/, 1)[0].replace(/:\d+(?::\d+)?$/, "");
	return CODE_FILE_EXTENSION.test(filePath);
}

// The markdown sanitizer behind Streamdown refuses every `file:` URL and
// prints "[blocked]" in its place, so a provider that links files as
// `file:///abs/path` loses the link before DCC can open it. The same path
// without the scheme reaches the link component, which already opens local
// files.
const FILE_LINK_TARGET = /\]\((<?)file:\/\/(?:localhost)?(?=\/|[a-zA-Z]:)/g;

export function unwrapFileLinkTargets(markdown: string) {
	return markdown.includes("file://") ? markdown.replace(FILE_LINK_TARGET, "]($1") : markdown;
}

/** Link title that marks a markdown link as an `@` workspace file mention. */
export const FILE_MENTION_LINK_TITLE = "dcc-file-mention";

/**
 * A `@relative/path` mention as a markdown link the anchor renders as a file
 * chip. The angle-bracket target keeps `(group)` and `[param]` segments intact;
 * the `./` prefix is what gets a relative target past the link sanitizer.
 */
export function fileMentionMarkdown(path: string) {
	const name = path.slice(path.lastIndexOf("/") + 1).replace(/[\\[\]*_`]/g, "\\$&");
	return `[@${name}](<./${path}> "${FILE_MENTION_LINK_TITLE}")`;
}

/** The workspace path back from a mention link's sanitized `href`. */
export function fileMentionPath(href: string) {
	let path = href;
	try {
		path = decodeURIComponent(href);
	} catch {
		// Keep the encoded form; it still names the file.
	}
	return path.replace(/^\.?\//, "");
}
