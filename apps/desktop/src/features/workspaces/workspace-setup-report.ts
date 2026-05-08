import type { WorkspaceSetupHint, WorkspaceSetupReport } from "@dcc/contracts";

type Translate = (key: string, options?: Record<string, string>) => string;

export function setupHintsDescription(
	t: Translate,
	setupHints: WorkspaceSetupHint[],
) {
	if (setupHints.length === 0) {
		return undefined;
	}

	return t("workspaceDialog.toastSetupSuggestions", {
		commands: setupHints.map((hint) => hint.command).join(" • "),
	});
}

export function setupReportDescription(
	t: Translate,
	setupReport: WorkspaceSetupReport,
	setupHints: WorkspaceSetupHint[],
) {
	const commands = setupHints.map((hint) => hint.command).join(" • ");
	const firstProblem = setupReport.steps.find(
		(step) => step.status === "warning" || step.status === "failed",
	);

	switch (setupReport.status) {
		case "completed":
			return t("workspaceDialog.toastSetupCompleted", { commands });
		case "warning":
			return (
				firstProblem?.detail ??
				setupReport.message ??
				t("workspaceDialog.toastSetupWarning", { commands })
			);
		case "failed":
			return (
				firstProblem?.detail ??
				setupReport.message ??
				t("workspaceDialog.toastSetupFailed", { commands })
			);
		default:
			return setupHintsDescription(t, setupHints);
	}
}
