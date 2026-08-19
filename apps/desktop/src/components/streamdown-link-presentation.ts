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
