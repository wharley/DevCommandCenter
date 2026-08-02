import { ArrowRight, ClipboardList } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import type { ParsedPlanContent } from "../plan-content";

type PlanSummaryCardProps = {
	plan: ParsedPlanContent;
	needsInput?: boolean;
	approved?: boolean;
	readOnly?: boolean;
	onOpen: () => void;
};

export function PlanSummaryCard({
	plan,
	needsInput = false,
	approved = false,
	readOnly = false,
	onOpen,
}: PlanSummaryCardProps) {
	const { t } = useTranslation("common");
	const stepCount = plan.steps.length;
	const title = plan.title === "Plan" ? t("planSurface.label") : plan.title;

	return (
		<article className="w-full rounded-2xl border border-border/70 bg-card/80 p-4 shadow-[0_10px_32px_rgba(0,0,0,0.05)]">
			<div className="flex items-start gap-3">
				<div className="flex size-9 shrink-0 items-center justify-center rounded-xl bg-primary/10 text-primary">
					<ClipboardList className="size-4.5" aria-hidden />
				</div>
				<div className="min-w-0 flex-1">
					<div className="flex flex-wrap items-center gap-2">
						<Badge
							variant={approved ? "success" : needsInput ? "outline" : "secondary"}
							className="rounded-md text-[10px] uppercase tracking-[0.08em]"
						>
							{approved
								? t("planSurface.approved")
								: needsInput
									? t("planSurface.needsInput")
									: t("planSurface.draft")}
						</Badge>
						{stepCount > 0 ? (
							<span className="text-[11px] text-muted-foreground">
								{t("planSurface.stepCount", { count: stepCount })}
							</span>
						) : null}
					</div>
					<h3 className="mt-2 text-sm font-semibold leading-5 text-foreground">
						{title}
					</h3>
					{plan.summary ? (
						<p className="mt-1 line-clamp-2 text-[12px] leading-5 text-muted-foreground">
							{plan.summary}
						</p>
					) : null}
				</div>
			</div>
			<div className="mt-4 flex items-center justify-between gap-3 border-t border-border/50 pt-3">
				<p className="text-[11px] leading-4 text-muted-foreground">
					{readOnly
						? t("planSurface.readOnlySummary")
						: approved
						? t("planSurface.approvedHint")
						: needsInput
						? t("planSurface.needsInputSummary")
						: t("planSurface.reviewBeforeExecution")}
				</p>
				<Button type="button" size="sm" className="shrink-0 gap-1.5" onClick={onOpen}>
					{readOnly
						? t("planSurface.viewPlan")
						: t("planSurface.reviewPlan")}
					<ArrowRight className="size-3.5" aria-hidden />
				</Button>
			</div>
		</article>
	);
}
