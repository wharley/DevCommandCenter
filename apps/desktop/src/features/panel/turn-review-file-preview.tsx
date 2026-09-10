import { useQuery } from "@tanstack/react-query";
import type { TurnReviewFile } from "@dcc/contracts";
import { Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { WorkspacePatchDiffLoader } from "@/features/editor/WorkspaceChangesDiffLoader";
import { loadTurnReviewFileDiff } from "@/lib/session-api";
import { Button } from "@/components/ui/button";

/** Fetch the immutable patch only while its preview is open. */
export function TurnReviewFilePreview({
	snapshotId,
	file,
}: {
	snapshotId: string;
	file: TurnReviewFile;
}) {
	const { t } = useTranslation("common");
	const query = useQuery({
		queryKey: ["turnReviewFileDiff", snapshotId, file.path],
		queryFn: () => loadTurnReviewFileDiff(snapshotId, file.path),
		enabled: !file.previewUnavailable,
		staleTime: Infinity,
	});
	if (file.previewUnavailable || query.data?.previewUnavailable) {
		return (
			<p className="flex h-full items-center justify-center p-4 text-center text-xs text-muted-foreground">
				{t("turnReview.previewUnavailable")}
			</p>
		);
	}
	if (query.isPending) {
		return (
			<div
				role="status"
				className="flex h-full items-center justify-center gap-2 text-xs text-muted-foreground"
			>
				<Loader2 className="size-4 animate-spin" />
				{t("turnReview.loading")}
			</div>
		);
	}
	if (query.isError) {
		return (
			<div className="flex h-full flex-col items-center justify-center gap-2 p-4 text-center text-xs">
				<p className="text-destructive">{t("turnReview.diffFailed")}</p>
				<Button
					size="sm"
					variant="outline"
					onClick={() => void query.refetch()}
				>
					{t("turnReview.timeline.retry")}
				</Button>
			</div>
		);
	}
	if (!query.data?.diff) {
		return (
			<p className="flex h-full items-center justify-center p-4 text-center text-xs text-muted-foreground">
				{t("turnReview.previewUnavailable")}
			</p>
		);
	}
	return (
		<WorkspacePatchDiffLoader
			path={file.path}
			patch={query.data.diff}
			className="h-full"
		/>
	);
}
