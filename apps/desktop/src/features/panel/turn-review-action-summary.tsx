import { useTranslation } from "react-i18next";
import { useCachedTurnReviewSummary } from "./turn-review-query";

export function TurnReviewActionSummary({
	sessionId,
	workspaceId,
}: {
	sessionId: string;
	workspaceId: string;
}) {
	const { t } = useTranslation("common");
	const review = useCachedTurnReviewSummary(sessionId, workspaceId);
	if (!review) return null;
	return (
		<span className="ml-auto truncate tabular-nums text-muted-foreground">
			{t(`turnReview.states.${review.state}`)} · {t("turnReview.fileCount", { count: review.files.length })} ·{" "}
			<span className="text-emerald-600">+{review.insertions}</span>{" "}
			<span className="text-destructive">−{review.deletions}</span>
		</span>
	);
}
