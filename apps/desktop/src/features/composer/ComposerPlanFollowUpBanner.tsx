import { memo } from "react";
import { CheckCircle2, ClipboardList, MessageCircleQuestion } from "lucide-react";
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
		const statusKey = approved
			? "composer.planFollowUp.approved"
			: needsInput
				? "composer.planFollowUp.needsInput"
				: "composer.planFollowUp.kicker";
		const StatusIcon = approved
			? CheckCircle2
			: needsInput
				? MessageCircleQuestion
				: ClipboardList;

		return (
			<div className="flex min-w-0 items-center gap-2 border-b border-border/40 bg-muted/10 px-3 py-2">
				<StatusIcon
					className={
						approved
							? "size-3.5 shrink-0 text-emerald-500"
							: "size-3.5 shrink-0 text-muted-foreground"
					}
					aria-hidden
				/>
				<p className="min-w-0 flex-1 truncate text-[12px] text-muted-foreground">
					<span className="font-medium text-foreground">{t(statusKey)}</span>
					<span className="px-1.5 text-border">·</span>
					<span>{planTitle ?? t("composer.planFollowUp.fallbackTitle")}</span>
				</p>
				<Button
					type="button"
					variant="ghost"
					size="sm"
					className="h-7 shrink-0 px-2.5 text-xs"
					onClick={onReviewPlan}
				>
					{t("composer.planFollowUp.openPlan")}
				</Button>
			</div>
		);
	},
);
