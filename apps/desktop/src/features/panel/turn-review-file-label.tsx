import type { TurnReviewFile } from "@dcc/contracts";
import { FileDiff, FileMinus2, FilePlus2, FileSymlink } from "lucide-react";
import { useTranslation } from "react-i18next";
import { hasTurnReviewLineStats } from "./turn-review.logic";

export function TurnReviewStats({
	insertions,
	deletions,
}: {
	insertions: number;
	deletions: number;
}) {
	return (
		<span className="dcc-turn-review-stats">
			<span>+{insertions}</span>
			<span>−{deletions}</span>
		</span>
	);
}

export function TurnReviewFileLabel({ file }: { file: TurnReviewFile }) {
	const { t } = useTranslation("common");
	const slash = file.path.lastIndexOf("/");
	const status =
		file.status === "A" || file.untracked
			? "added"
			: file.status === "D"
				? "deleted"
				: file.status.startsWith("R")
					? "renamed"
					: "modified";
	const Icon =
		status === "added"
			? FilePlus2
			: status === "deleted"
				? FileMinus2
				: status === "renamed"
					? FileSymlink
					: FileDiff;
	return (
		<>
			<Icon size={15} aria-hidden className="dcc-turn-review-file-icon" />
			<span className="dcc-turn-review-path">
				<span className="dcc-turn-review-filename">
					{file.path.slice(slash + 1)}
				</span>
				{slash >= 0 && (
					<span className="dcc-turn-review-folder">
						{file.path.slice(0, slash)}
					</span>
				)}
			</span>
			<span className="dcc-turn-review-status" data-status={status}>
				{t(`turnReview.timeline.status.${status}`)}
			</span>
			{hasTurnReviewLineStats(file) && <TurnReviewStats {...file} />}
		</>
	);
}
