import { useState } from "react";
import { AlertTriangle, Check, CircleHelp, Copy } from "lucide-react";
import { toast } from "sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { saveMissionValidation } from "@/lib/workspace-api";
import type {
	MissionValidationStatus,
	ParsedMissionValidationReport,
} from "@/features/spec/mission-spec-content";

type MissionValidationCardProps = {
	report: ParsedMissionValidationReport;
	workspacePath?: string | null;
	showSaveAction?: boolean;
};

function statusIcon(status: MissionValidationStatus) {
	if (status === "PASS") {
		return <Check className="size-3.5 text-emerald-500" strokeWidth={2} />;
	}
	if (status === "FAIL") {
		return <AlertTriangle className="size-3.5 text-destructive" strokeWidth={2} />;
	}
	return <CircleHelp className="size-3.5 text-amber-500" strokeWidth={2} />;
}

function statusClassName(status: MissionValidationStatus) {
	if (status === "PASS") {
		return "border-emerald-500/25 bg-emerald-500/5";
	}
	if (status === "FAIL") {
		return "border-destructive/25 bg-destructive/5";
	}
	return "border-amber-500/25 bg-amber-500/5";
}

export function MissionValidationCard({
	report,
	workspacePath,
	showSaveAction = true,
}: MissionValidationCardProps) {
	const [isSaving, setIsSaving] = useState(false);
	const passCount = report.criteria.filter((criterion) => criterion.status === "PASS").length;
	const failCount = report.criteria.filter((criterion) => criterion.status === "FAIL").length;
	const unknownCount = report.criteria.filter(
		(criterion) => criterion.status === "UNKNOWN",
	).length;

	const handleCopyJson = async () => {
		try {
			await navigator.clipboard.writeText(report.rawJson);
			toast.success("Validation JSON copied");
		} catch (error) {
			toast.error(
				error instanceof Error
					? error.message
					: "Unable to copy validation JSON.",
			);
		}
	};

	const handleSave = async () => {
		const root = workspacePath?.trim();
		const specRelativePath = report.specRelativePath?.trim();
		if (!root || !specRelativePath) {
			toast.error("Validation cannot be saved without workspace and spec path.");
			return;
		}

		setIsSaving(true);
		try {
			const result = await saveMissionValidation({
				workspaceRoot: root,
				specRelativePath,
				reportJson: report.rawJson,
			});
			toast.success("Validation saved", {
				description: result.relativePath,
			});
		} catch (error) {
			toast.error(
				error instanceof Error
					? error.message
					: "Unable to save validation report.",
			);
		} finally {
			setIsSaving(false);
		}
	};

	return (
		<div className="rounded-[22px] border border-border/70 bg-card/75 p-4 shadow-[0_12px_40px_rgba(0,0,0,0.06)] backdrop-blur-sm">
			<div className="flex flex-wrap items-start justify-between gap-3">
				<div className="min-w-0">
					<div className="flex items-center gap-2">
						<Badge
							variant="secondary"
							className="rounded-md px-2 py-0 text-[10px] font-semibold uppercase tracking-[0.08em]"
						>
							Validation
						</Badge>
						<p className="truncate text-sm font-medium text-foreground">
							Mission acceptance criteria
						</p>
					</div>
					<p className="mt-1 text-[11px] leading-5 text-muted-foreground">
						{passCount} pass · {failCount} fail · {unknownCount} unknown
					</p>
				</div>
				<div className="flex items-center gap-1.5">
					{showSaveAction ? (
						<Button
							type="button"
							variant="outline"
							size="sm"
							className="h-7 rounded-lg px-2 text-[11px]"
							disabled={isSaving || !workspacePath || !report.specRelativePath}
							onClick={() => void handleSave()}
						>
							{isSaving ? "Saving..." : "Save verdict"}
						</Button>
					) : null}
					<Button
						type="button"
						variant="outline"
						size="icon-xs"
						aria-label="Copy validation JSON"
						onClick={() => void handleCopyJson()}
					>
						<Copy className="size-3.5" aria-hidden />
					</Button>
				</div>
			</div>

			{report.summary ? (
				<p className="mt-3 text-[12px] leading-5 text-muted-foreground">
					{report.summary}
				</p>
			) : null}

			<div className="mt-4 grid gap-1.5">
				{report.criteria.map((criterion) => (
					<div
						key={criterion.id}
						className={cn(
							"rounded-xl border px-2.5 py-2",
							statusClassName(criterion.status),
						)}
					>
						<div className="flex items-center gap-2">
							{statusIcon(criterion.status)}
							<span className="font-mono text-[11px] font-semibold text-foreground">
								{criterion.id}
							</span>
							<span className="text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
								{criterion.status}
							</span>
						</div>
						{criterion.evidence ? (
							<p className="mt-1 pl-5 text-[12px] leading-5 text-foreground">
								{criterion.evidence}
							</p>
						) : null}
						{criterion.nextAction ? (
							<p className="mt-1 pl-5 text-[11px] leading-5 text-muted-foreground">
								Next: {criterion.nextAction}
							</p>
						) : null}
					</div>
				))}
			</div>
		</div>
	);
}
