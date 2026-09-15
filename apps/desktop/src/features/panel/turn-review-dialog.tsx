import { useWorkspaceDeliveryBusy } from "@/features/commit/workspace-delivery-busy";
import { TurnReviewDelivery } from "./turn-review-delivery";
import { useCallback, useId } from "react";
import type { TurnReviewSummary } from "@dcc/contracts";
import { ArrowUpRight, ChevronLeft, ChevronRight, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
	Dialog,
	DialogClose,
	DialogContent,
	DialogDescription,
	DialogTitle,
} from "@/components/ui/dialog";
import { TurnReviewFilePreview } from "./turn-review-file-preview";
import { TurnReviewFileLabel, TurnReviewStats } from "./turn-review-file-label";
import { hasTurnReviewLineStats } from "./turn-review.logic";
import type { DiffAnnotationRequest } from "@/features/editor/diff-annotation";

export function TurnReviewDialog({
	review,
	workspaceRoot,
	selectedPath,
	onSelect,
	onClose,
	onReview,
	onAddToChat,
	returnFocus,
}: {
	review: TurnReviewSummary;
	workspaceRoot?: string | null;
	selectedPath: string | null;
	onSelect: (path: string) => void;
	onClose: () => void;
	onReview: () => void;
	onAddToChat: (requests: DiffAnnotationRequest[]) => void;
	returnFocus: () => void;
}) {
	const { t } = useTranslation("common");
	const previewId = useId();
	const deliveryBusy = useWorkspaceDeliveryBusy(workspaceRoot);
	const revealSelectedFile = useCallback((node: HTMLButtonElement | null) => {
		node?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
	}, []);
	const index = Math.max(
		0,
		review.files.findIndex((file) => file.path === selectedPath),
	);
	const file = review.files[index];
	if (!file) return null;
	return (
		<Dialog
			open={selectedPath !== null}
			onOpenChange={(open) => {
				if (!open && !deliveryBusy) onClose();
			}}
		>
			<DialogContent
				className="dcc-turn-review-dialog"
				showCloseButton={false}
				onCloseAutoFocus={(event) => {
					event.preventDefault();
					returnFocus();
				}}
			>
				<header className="dcc-turn-review-dialog-heading">
					<div>
						<DialogTitle>{t("turnReview.timeline.title")}</DialogTitle>
						<DialogDescription>
							{t("turnReview.timeline.fileCount", {
								count: review.files.length,
							})}{" "}
							·{" "}
							{t(
								review.turnOutcome === "aborted"
									? "turnReview.timeline.partial"
									: "turnReview.timeline.thisTurn",
							)}
						</DialogDescription>
					</div>
					{review.files.every(hasTurnReviewLineStats) && (
						<TurnReviewStats {...review} />
					)}
					<DialogClose
						disabled={deliveryBusy}
						className="dcc-turn-review-icon-button"
						aria-label={t("turnReview.timeline.close")}
					>
						<X size={17} aria-hidden />
					</DialogClose>
				</header>
				<div className="dcc-turn-review-dialog-body">
					<nav
						className="dcc-turn-review-dialog-files"
						aria-label={t("turnReview.fileList")}
					>
						{review.files.map((item) => (
							<button
								key={item.path}
								ref={item.path === file.path ? revealSelectedFile : undefined}
								type="button"
								className="dcc-turn-review-file-trigger"
								title={item.path}
								aria-current={item.path === file.path ? "true" : undefined}
								aria-controls={previewId}
								onClick={() => onSelect(item.path)}
							>
								<TurnReviewFileLabel file={item} />
							</button>
						))}
					</nav>
					<div className="dcc-turn-review-dialog-main">
						<div className="dcc-turn-review-dialog-toolbar">
							<span className="dcc-turn-review-selected-path" title={file.path}>
								{file.path}
							</span>
							<span className="dcc-turn-review-position">
								{index + 1} / {review.files.length}
							</span>
							<button
								type="button"
								className="dcc-turn-review-icon-button"
								aria-label={t("turnReview.timeline.previousFile")}
								disabled={index === 0}
								onClick={() => onSelect(review.files[index - 1]!.path)}
							>
								<ChevronLeft size={16} aria-hidden />
							</button>
							<button
								type="button"
								className="dcc-turn-review-icon-button"
								aria-label={t("turnReview.timeline.nextFile")}
								disabled={index === review.files.length - 1}
								onClick={() => onSelect(review.files[index + 1]!.path)}
							>
								<ChevronRight size={16} aria-hidden />
							</button>
						</div>
						<div
							id={previewId}
							className="dcc-turn-review-dialog-preview"
							role="region"
							aria-label={file.path}
						>
							<TurnReviewFilePreview
								key={`${review.snapshotId}:${file.path}`}
								snapshotId={review.snapshotId}
								file={file}
								onAddToChat={onAddToChat}
							/>
						</div>
					</div>
				</div>
				{selectedPath !== null && workspaceRoot && (
					<TurnReviewDelivery
						workspaceRoot={workspaceRoot}
						onReview={onReview}
					/>
				)}
				<footer className="dcc-turn-review-dialog-footer">
					<span>
						{t(
							review.diffTruncated
								? "turnReview.diffTruncatedNotice"
								: "turnReview.timeline.selectedTurnHint",
						)}
					</span>
					<button
						type="button"
						disabled={deliveryBusy}
						onClick={() => {
							onClose();
							onReview();
						}}
					>
						{t("turnReview.timeline.currentChanges")}
						<ArrowUpRight size={13} aria-hidden />
					</button>
				</footer>
			</DialogContent>
		</Dialog>
	);
}
