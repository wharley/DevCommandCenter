export function inferProjectIdFromWorkspaceRoot(workspaceRoot: string): string {
	const normalized = workspaceRoot.replaceAll("\\", "/").replace(/\/+$/, "");
	const lastSegment = normalized.split("/").filter(Boolean).pop() ?? "";

	return sanitizeProjectIdSegment(lastSegment);
}

function sanitizeProjectIdSegment(value: string): string {
	const sanitized = value
		.trim()
		.toLowerCase()
		.replace(/[^a-z0-9._-]+/g, "-")
		.replace(/-+/g, "-")
		.replace(/^[-_.]+|[-_.]+$/g, "");

	return sanitized.length > 0 ? sanitized : "project";
}
