/** A selected source range emitted by file and diff review surfaces. */
export type DiffAnnotationPayload = {
	/** "original" = deleted/old side, "modified" = added/new side. */
	side: "original" | "modified";
	/** One-based inclusive source line range. */
	startLine: number;
	endLine: number;
	snippet: string;
	/** Viewport coordinates of the trigger button, for anchoring an overlay. */
	anchor: { top: number; left: number };
};

export type DiffMachineAnnotation = {
	source: "coderabbit" | "forge-review";
	id?: string;
	severity: "critical" | "major" | "minor" | "trivial" | "info" | "unknown";
	side: "original" | "modified";
	startLine: number;
	endLine: number;
	title: string;
};
