import { memo } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";

type ComposerPlanFollowUpBannerProps = {
	planTitle: string | null;
	onDelegatePlan: () => void;
	onImplementInNewThread: () => void;
	onOpenPlanSidebar: () => void;
};

export const ComposerPlanFollowUpBanner = memo(
	function ComposerPlanFollowUpBanner({
		planTitle,
		onDelegatePlan,
		onImplementInNewThread,
		onOpenPlanSidebar,
	}: ComposerPlanFollowUpBannerProps) {
		const { t } = useTranslation("common");

		return (
			<div className="border-b border-border/40 bg-muted/10 px-4 py-3 sm:px-5 sm:py-4">
				<div className="flex flex-wrap items-center justify-between gap-3">
					<div className="min-w-0 flex-1">
						<p className="text-[11px] font-semibold uppercase tracking-[0.2em] text-muted-foreground">
							{t("composer.planFollowUp.kicker")}
						</p>
						<p className="truncate text-sm font-medium text-foreground">
							{planTitle ?? t("composer.planFollowUp.fallbackTitle")}
						</p>
					</div>
					<div className="flex shrink-0 items-center gap-2">
						<Button
							type="button"
							variant="outline"
							size="sm"
							className="rounded-full"
							onClick={onOpenPlanSidebar}
						>
							{t("composer.planFollowUp.openPlan")}
						</Button>
						<Button
							type="button"
							size="sm"
							className="rounded-full"
							variant="outline"
							onClick={onDelegatePlan}
						>
							{t("composer.planFollowUp.delegatePlan")}
						</Button>
						<Button
							type="button"
							size="sm"
							className="rounded-full"
							onClick={onImplementInNewThread}
						>
							{t("composer.planFollowUp.implementInNewThread")}
						</Button>
					</div>
				</div>
			</div>
		);
	},
);
