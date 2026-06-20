import type {
	CodeRabbitFinding,
	CodeRabbitReviewType,
} from "@dcc/contracts";
import type { WorkspaceGitPreviewMachineAnnotation } from "./workspace-git-file-preview";

export function lineLabel(finding: CodeRabbitFinding): string | null {
	if (!finding.startLine) {
		return null;
	}
	if (!finding.endLine || finding.endLine === finding.startLine) {
		return `L${finding.startLine}`;
	}
	return `L${finding.startLine}-${finding.endLine}`;
}

export function findingTitle(finding: CodeRabbitFinding): string {
	return (
		finding.comment?.trim() ||
		finding.codegenInstructions?.trim() ||
		finding.suggestions[0]?.trim() ||
		"CodeRabbit finding"
	);
}

export function buildCodeRabbitComposerPrompt({
	findings,
	reviewType,
	completedAt,
	isStale,
}: {
	findings: CodeRabbitFinding[];
	reviewType: CodeRabbitReviewType;
	completedAt?: string | null;
	isStale: boolean;
}): string {
	const parts: string[] = [
		`Fix the selected CodeRabbit review finding${findings.length === 1 ? "" : "s"}.`,
		"",
		`Review scope: ${reviewType}`,
		completedAt ? `Review completed at: ${completedAt}` : null,
		isStale
			? "Note: the workspace diff changed after this review; verify every location before editing."
			: null,
		"",
	].filter((part): part is string => part !== null);

	findings.forEach((finding, index) => {
		parts.push(`${index + 1}. [${finding.severity}] ${findingLocation(finding)}`);
		if (finding.comment?.trim()) {
			parts.push(`Comment: ${finding.comment.trim()}`);
		}
		if (finding.codegenInstructions?.trim()) {
			parts.push(`Codegen instructions: ${finding.codegenInstructions.trim()}`);
		}
		const suggestions = finding.suggestions
			.map((suggestion) => suggestion.trim())
			.filter((suggestion) => suggestion.length > 0);
		if (suggestions.length > 0) {
			parts.push("Suggestions:");
			for (const suggestion of suggestions) {
				parts.push(`- ${suggestion}`);
			}
		}
		parts.push("");
	});

	parts.push(
		"Apply the fixes in the workspace, keep the changes scoped to these findings, and mention any finding you intentionally leave unchanged.",
	);
	return parts.join("\n");
}

export function buildMachineAnnotationsForPath(
	path: string,
	findings: CodeRabbitFinding[],
): WorkspaceGitPreviewMachineAnnotation[] {
	return findings
		.filter((finding) => finding.path === path && finding.startLine != null)
		.map((finding) => ({
			source: "coderabbit" as const,
			severity: finding.severity,
			side: coderabbitSide(finding.side),
			startLine: finding.startLine!,
			endLine: finding.endLine ?? finding.startLine!,
			title: findingTitle(finding),
		}));
}

function findingLocation(finding: CodeRabbitFinding): string {
	const label = lineLabel(finding);
	return label ? `${finding.path}:${label}` : finding.path;
}

function coderabbitSide(value: string | null): "original" | "modified" {
	const normalized = value?.trim().toLowerCase();
	return normalized === "original" || normalized === "old" || normalized === "left"
		? "original"
		: "modified";
}
