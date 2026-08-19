import type { WorkspaceSummary } from "./types";

export function workspaceDiskUsageIds(
	workspaces: readonly WorkspaceSummary[],
): string[] {
	return [
		...new Set(
			workspaces.flatMap((workspace) =>
				workspace.memberWorkspaceIds?.length
					? workspace.memberWorkspaceIds
					: [workspace.id],
			),
		),
	];
}

export function formatDiskBytes(bytes: number, locale?: string): string {
	const safeBytes = Number.isFinite(bytes) ? Math.max(0, bytes) : 0;
	const units = ["B", "KB", "MB", "GB", "TB"] as const;
	let value = safeBytes;
	let unitIndex = 0;
	while (value >= 1024 && unitIndex < units.length - 1) {
		value /= 1024;
		unitIndex += 1;
	}

	return `${new Intl.NumberFormat(locale, {
		maximumFractionDigits: unitIndex === 0 || value >= 10 ? 0 : 1,
	}).format(value)} ${units[unitIndex]}`;
}
