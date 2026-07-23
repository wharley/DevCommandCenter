import { memo } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";

type ComposerPlanFollowUpBannerProps = {
	planTitle: string | null;
	needsInput: boolean;
	approved: boolean;
	onReviewPlan: () => void;
};

export const ComposerPlanFollowUpBanner = memo(
	function ComposerPlanFollowUpBanner({
		planTitle,
		needsInput,
		approved,
		onReviewPlan,
	}: ComposerPlanFollowUpBannerProps) {
		const { t } = useTranslation("common");

		return (
			<div className="border-b border-border/40 bg-muted/10 px-4 py-3 sm:px-5 sm:py-4">
				<div className="flex flex-wrap items-center justify-between gap-3">
					<div className="min-w-0 flex-1">
						<p className="text-[11px] font-semibold uppercase tracking-[0.2em] text-muted-foreground">
							{t(
								approved
									? "composer.planFollowUp.approved"
									: needsInput
									? "composer.planFollowUp.needsInput"
									: "composer.planFollowUp.kicker",
							)}
						</p>
						<p className="truncate text-sm font-medium text-foreground">
							{planTitle ?? t("composer.planFollowUp.fallbackTitle")}
						</p>
					</div>
					<Button
						type="button"
						size="sm"
						className="shrink-0 rounded-full"
						onClick={onReviewPlan}
					>
						{t("composer.planFollowUp.openPlan")}
					</Button>
				</div>
			</div>
		);
	},
);
