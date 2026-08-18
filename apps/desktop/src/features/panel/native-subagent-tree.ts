import type { WorkspaceMessageAnnotation } from "@/features/sessions/session-thread-history.logic";

export type NativeSubagentAnnotation = Extract<
	WorkspaceMessageAnnotation,
	{ type: "native-subagent" }
>;

export type NativeSubagentTreeNode = {
	key: string;
	label: string;
	annotation?: NativeSubagentAnnotation;
	children: NativeSubagentTreeNode[];
};

export type NativeSubagentTreeProjection = {
	roots: NativeSubagentTreeNode[];
	ungrouped: NativeSubagentAnnotation[];
	hierarchicalCount: number;
};

export type NativeSubagentControlAvailability = {
	canSteer: boolean;
	canInterrupt: boolean;
};

export type NativeSubagentDisplayStatus =
	| NativeSubagentAnnotation["status"]
	| "settled";

const MAX_PATH_CHARS = 1_024;
const MAX_PATH_SEGMENTS = 16;
const MAX_SEGMENT_CHARS = 128;

export function parseNativeSubagentPath(path: string | null | undefined) {
	const normalized = path?.trim();
	if (!normalized || normalized.length > MAX_PATH_CHARS) return null;
	const withoutLeadingSlash = normalized.startsWith("/")
		? normalized.slice(1)
		: normalized;
	const segments = withoutLeadingSlash.split("/");
	if (
		segments.length < 2 ||
		segments.length > MAX_PATH_SEGMENTS ||
		segments[0] !== "root" ||
		segments.some(
			(segment) =>
				!segment ||
				segment === "." ||
				segment === ".." ||
				segment.length > MAX_SEGMENT_CHARS ||
				[...segment].some((character) => /\p{Cc}/u.test(character)),
		)
	) {
		return null;
	}
	return segments;
}

function canonicalPath(annotation: NativeSubagentAnnotation) {
	if (parseNativeSubagentPath(annotation.path)) return annotation.path;
	// Historical events stored Codex's agentPath in `name`. Keep those
	// sessions eligible for the tree without changing their persisted data.
	if (parseNativeSubagentPath(annotation.name)) return annotation.name;
	return null;
}

export function projectNativeSubagentTree(
	annotations: NativeSubagentAnnotation[],
): NativeSubagentTreeProjection {
	const roots: NativeSubagentTreeNode[] = [];
	const ungrouped: NativeSubagentAnnotation[] = [];
	let hierarchicalCount = 0;

	for (const annotation of annotations) {
		// Codex reports the primary thread through the same structured activity
		// channel with the canonical path `/root`. It belongs to the explicit
		// main-agent row above the tree, never to the child-card collection.
		if (annotation.path?.trim().replace(/^\//, "") === "root") {
			continue;
		}
		const segments = parseNativeSubagentPath(canonicalPath(annotation));
		if (!segments) {
			ungrouped.push(annotation);
			continue;
		}

		hierarchicalCount += 1;
		let siblings = roots;
		let key = "root";
		for (const segment of segments.slice(1)) {
			key = `${key}/${segment}`;
			let node = siblings.find((candidate) => candidate.key === key);
			if (!node) {
				node = { key, label: segment, children: [] };
				siblings.push(node);
			}
			siblings = node.children;
			if (key === segments.join("/")) node.annotation = annotation;
		}
	}

	return { roots, ungrouped, hierarchicalCount };
}

export function nativeSubagentDisplayStatus(
	annotation: NativeSubagentAnnotation,
	parentStreaming: boolean | undefined,
): NativeSubagentDisplayStatus {
	// Historical Codex sessions may predate terminal child projection. Once
	// their parent turn has settled, do not keep claiming that the child is
	// actively working; `settled` is intentionally neutral about its outcome.
	return annotation.status === "running" && !parentStreaming
		? "settled"
		: annotation.status;
}

export function nativeSubagentControlAvailability(
	annotation: NativeSubagentAnnotation,
	options: {
		sessionId?: string | null;
		parentStreaming?: boolean;
		supportsSteering?: boolean;
		supportsInterrupt?: boolean;
	},
): NativeSubagentControlAvailability {
	const hasLiveTarget = Boolean(
		options.sessionId?.trim() &&
			options.parentStreaming &&
			annotation.status === "running" &&
			annotation.agentThreadId?.trim(),
	);
	return {
		canSteer: hasLiveTarget && Boolean(options.supportsSteering),
		canInterrupt: hasLiveTarget && Boolean(options.supportsInterrupt),
	};
}
