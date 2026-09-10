import "@/features/review/review-surfaces.css";
import { useId, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
	ArrowUpRight,
	ChevronDown,
	ChevronRight,
	FileDiff,
	GitCompareArrows,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { TurnReviewFilePreview } from "./turn-review-file-preview";
import {
	turnReviewQueryOptions,
	type TurnReviewTarget,
} from "./turn-review-query";

export function TurnReviewTimelineCard({
	target,
	onReview,
	onInteraction,
}: {
	target: TurnReviewTarget;
	onReview: (target: TurnReviewTarget) => void;
	onInteraction?: () => void;
}) {
	const { t } = useTranslation("common");
	const id = useId();
	const [expanded, setExpanded] = useState(false);
	const [selectedFile, setSelectedFile] = useState<string | null>(null);
	const query = useQuery(turnReviewQueryOptions(target));
	const review = query.data;
	// Never substitute another turn, including when connected to an older backend.
	if (
		!review ||
		review.turnId !== target.turnId ||
		review.sessionId !== target.sessionId ||
		review.workspaceId !== target.workspaceId ||
		review.state === "collecting" ||
		review.files.length === 0
	)
		return null;
	return (
		<section
			className="dcc-turn-review-card"
			aria-label={t("turnReview.timeline.title")}
			data-turn-review-id={target.turnId}
		>
			<div className="dcc-turn-review-summary">
				<GitCompareArrows
					className="dcc-turn-review-mark"
					size={16}
					aria-hidden
				/>
				<div className="dcc-turn-review-heading">
					<span>
						{t("turnReview.timeline.fileCount", { count: review.files.length })}
					</span>
					<span className="dcc-turn-review-stats">
						<span>+{review.insertions}</span>
						<span>−{review.deletions}</span>
					</span>
				</div>
				<span className="dcc-turn-review-caption">
					{t(
						review.turnOutcome === "aborted"
							? "turnReview.timeline.partial"
							: "turnReview.timeline.thisTurn",
					)}
				</span>
			</div>
			<div className="dcc-turn-review-actions">
				<button
					type="button"
					aria-expanded={expanded}
					aria-controls={`${id}-files`}
					onClick={() => {
						onInteraction?.();
						setExpanded(!expanded);
						setSelectedFile(null);
					}}
				>
					<ChevronDown
						size={14}
						className={expanded ? "rotate-180" : ""}
						aria-hidden
					/>
					{t(
						expanded
							? "turnReview.timeline.hideFiles"
							: "turnReview.timeline.showFiles",
					)}
				</button>
				<button
					type="button"
					className="dcc-turn-review-open"
					onClick={() =>
						onReview({ ...target, filePath: selectedFile ?? undefined })
					}
				>
					{t("turnReview.timeline.review")}
					<ArrowUpRight size={14} aria-hidden />
				</button>
			</div>
			<div
				id={`${id}-files`}
				hidden={!expanded}
				className="dcc-turn-review-files"
			>
				{expanded &&
					review.files.map((file, index) => {
						const open = selectedFile === file.path;
						return (
							<div key={file.path} className="dcc-turn-review-file">
								<button
									type="button"
									className="dcc-turn-review-file-trigger"
									aria-expanded={open}
									aria-controls={`${id}-diff-${index}`}
									title={file.path}
									onClick={() => {
										onInteraction?.();
										setSelectedFile(open ? null : file.path);
									}}
								>
									{open ? (
										<ChevronDown size={14} aria-hidden />
									) : (
										<ChevronRight size={14} aria-hidden />
									)}
									<FileDiff size={14} aria-hidden />
									<span className="dcc-turn-review-path">{file.path}</span>
									<span className="dcc-turn-review-stats">
										<span>+{file.insertions}</span>
										<span>−{file.deletions}</span>
									</span>
								</button>
								<div id={`${id}-diff-${index}`} hidden={!open}>
									{open && (
										<div
											className="dcc-turn-review-preview"
											style={{
												height: file.previewUnavailable
													? 96
													: Math.min(
															300,
															Math.max(
																140,
																(file.insertions + file.deletions + 5) * 18 +
																	32,
															),
														),
											}}
										>
											<TurnReviewFilePreview
												snapshotId={review.snapshotId}
												file={file}
											/>
										</div>
									)}
								</div>
							</div>
						);
					})}
			</div>
		</section>
	);
}
