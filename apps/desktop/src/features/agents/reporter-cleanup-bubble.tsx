import { Loader2 } from "lucide-react";
import { useRef } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { formatDiskBytes } from "@/features/workspaces/workspace-disk-usage";
import { ReporterThought } from "./chronicler-recap-bubble";

export type ReporterCleanupAlert = {
	totalBytes: number;
	safeCount: number;
	safeBytes: number;
	/** Set while the reporter checks (`total` 0) and then deletes the safe tasks. */
	progress: { done: number; total: number } | null;
};

/**
 * The reporter pointing at the space completed tasks take, once they pass the
 * person's limit: review the list, or delete only what is safe.
 */
export function ReporterCleanupBubble({
	name,
	alert: liveAlert,
	onReview,
	onCleanSafe,
	onDismiss,
}: {
	name: string;
	alert: ReporterCleanupAlert;
	onReview: () => void;
	onCleanSafe: () => void;
	onDismiss: () => void;
}) {
	const { t, i18n } = useTranslation("common");
	// While tasks are being deleted the totals shrink under the bubble; it
	// keeps saying what it said when the person clicked.
	const settled = useRef(liveAlert);
	if (!liveAlert.progress) settled.current = liveAlert;
	const alert = liveAlert.progress
		? { ...settled.current, progress: liveAlert.progress }
		: liveAlert;
	const size = (bytes: number) => formatDiskBytes(bytes, i18n.resolvedLanguage);
	const text =
		alert.safeCount > 0
			? t("agents.cleanup.bubble.safe", {
					total: size(alert.totalBytes),
					count: alert.safeCount,
					safe: size(alert.safeBytes),
				})
			: t("agents.cleanup.bubble.noneSafe", { total: size(alert.totalBytes) });
	const cleaning = alert.progress !== null;
	return (
		<ReporterThought
			name={name}
			text={text}
			dismissLabel={t("agents.offer.dismiss")}
			dismissDisabled={cleaning}
			onDismiss={onDismiss}
		>
			<Button type="button" size="sm" variant="outline" disabled={cleaning} onClick={onReview}>
				<span>{t("agents.cleanup.bubble.review")}</span>
			</Button>
			{alert.safeCount > 0 && (
				<Button type="button" size="sm" disabled={cleaning} onClick={onCleanSafe}>
					{alert.progress ? (
						<>
							<Loader2 className="size-3.5 animate-spin" aria-hidden />
							<span>
								{alert.progress.total === 0
									? t("agents.cleanup.bubble.checking")
									: t("agents.cleanup.bubble.cleaning", alert.progress)}
							</span>
						</>
					) : (
						<span>{t("agents.cleanup.bubble.cleanSafe")}</span>
					)}
				</Button>
			)}
		</ReporterThought>
	);
}
