import "@/features/review/review-surfaces.css";
import { useId, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { ChevronDown, ChevronRight } from "lucide-react";
import { useTranslation } from "react-i18next";
import { TurnReviewDialog } from "./turn-review-dialog";
import { TurnReviewFileLabel, TurnReviewStats } from "./turn-review-file-label";
import { hasTurnReviewLineStats } from "./turn-review.logic";
import { dispatchWorkspaceDiffAnnotation } from "@/features/editor/workspace-diff-annotation-command";
import {
	turnReviewQueryOptions,
	type TurnReviewTarget,
} from "./turn-review-query";

const VISIBLE_FILES = 3;

export function TurnReviewTimelineCard({
	target,
	onReview,
	onInteraction,
	workspaceRoot,
}: {
	target: TurnReviewTarget;
	onReview: () => void;
	onInteraction?: () => void;
	workspaceRoot?: string | null;
}) {
	const { t } = useTranslation("common");
	const id = useId();
	const [expanded, setExpanded] = useState(false);
	const [selectedFile, setSelectedFile] = useState<string | null>(null);
	const triggerRef = useRef<HTMLButtonElement | null>(null);
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
	const openReview = (path: string, trigger: HTMLButtonElement) => {
		onInteraction?.();
		triggerRef.current = trigger;
		setSelectedFile(path);
	};
	return (
		<section
			className="dcc-turn-review-card"
			aria-label={t("turnReview.timeline.title")}
			data-turn-review-id={target.turnId}
		>
			<div className="dcc-turn-review-summary">
				<div className="dcc-turn-review-heading">
					<span>
						{t("turnReview.timeline.fileCount", { count: review.files.length })}
					</span>
					{review.files.every(hasTurnReviewLineStats) && (
						<TurnReviewStats {...review} />
					)}
				</div>
				<button
					type="button"
					className="dcc-turn-review-open"
					aria-haspopup="dialog"
					onClick={(event) =>
						openReview(review.files[0]!.path, event.currentTarget)
					}
				>
					{t("turnReview.timeline.review")}
					<ChevronRight size={14} aria-hidden />
				</button>
			</div>
			<div id={`${id}-files`} className="dcc-turn-review-files">
				{(expanded ? review.files : review.files.slice(0, VISIBLE_FILES)).map(
					(file) => (
						<div key={file.path} className="dcc-turn-review-file">
							<button
								type="button"
								className="dcc-turn-review-file-trigger"
								aria-haspopup="dialog"
								title={file.path}
								onClick={(event) => openReview(file.path, event.currentTarget)}
							>
								<TurnReviewFileLabel file={file} />
								<ChevronRight
									size={13}
									aria-hidden
									className="dcc-turn-review-file-chevron"
								/>
							</button>
						</div>
					),
				)}
			</div>
			<div className="dcc-turn-review-actions">
				<span className="dcc-turn-review-caption">
					{t(
						review.turnOutcome === "aborted"
							? "turnReview.timeline.partial"
							: "turnReview.timeline.thisTurn",
					)}
				</span>
				{review.files.length > VISIBLE_FILES && (
					<button
						type="button"
						aria-expanded={expanded}
						aria-controls={`${id}-files`}
						onClick={() => {
							onInteraction?.();
							setExpanded(!expanded);
						}}
					>
						{t(
							expanded
								? "turnReview.timeline.showLess"
								: "turnReview.timeline.moreFiles",
							{ count: review.files.length - VISIBLE_FILES },
						)}
						<ChevronDown
							size={13}
							className={expanded ? "rotate-180" : ""}
							aria-hidden
						/>
					</button>
				)}
			</div>
			<TurnReviewDialog
				workspaceRoot={workspaceRoot}
				review={review}
				selectedPath={selectedFile}
				onSelect={setSelectedFile}
				onClose={() => setSelectedFile(null)}
				onReview={onReview}
				returnFocus={() => triggerRef.current?.focus()}
				onAddToChat={(requests) => {
					onInteraction?.();
					setSelectedFile(null);
					dispatchWorkspaceDiffAnnotation({
						workspaceId: target.workspaceId,
						targetSessionId: target.sessionId,
						destination: "composer",
						requests,
					});
				}}
			/>
		</section>
	);
}
