import codeReviewControlsCss from "./code-review-controls.css?inline";
import "./code-review-controls.css";
import type { SelectedLineRange } from "@pierre/diffs";
import { CodeView, type CodeViewHandle } from "@pierre/diffs/react";
import { MessageSquarePlus } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useAppearance } from "@/components/theme-provider";
import { workspaceDiffContentHash } from "./workspace-changes-diff.logic";
import { workspaceDiffViewOptions } from "./workspace-diff-view-options";
import {
	parseWorkspacePatch,
	patchSelectionRequests,
} from "./workspace-patch-diff.logic";
import type { DiffAnnotationRequest } from "./diff-annotation";
import { readPatchTextSelection } from "./patch-text-selection";

export type WorkspacePatchDiffProps = {
	path: string;
	patch: string;
	className?: string;
	onAddToChat?: (requests: DiffAnnotationRequest[]) => void;
};

const ITEM_ID = "dcc-workspace-patch";

/** Renders one captured Git patch with the same read-only visual system as the dock. */
export default function WorkspacePatchDiff({
	path,
	patch,
	className,
	onAddToChat,
}: WorkspacePatchDiffProps) {
	const { t } = useTranslation("common");
	const { theme } = useAppearance();
	const containerRef = useRef<HTMLDivElement>(null);
	const viewRef = useRef<CodeViewHandle<undefined>>(null);
	const resetFrameRef = useRef<number | null>(null);
	const [selectedLines, setSelectedLines] = useState<{
		id: string;
		range: SelectedLineRange;
	} | null>(null);
	const [textSelection, setTextSelection] = useState<SelectedLineRange | null>(null);
	const patchHash = useMemo(() => workspaceDiffContentHash(patch), [patch]);
	const fileDiff = useMemo(() => parseWorkspacePatch(patch), [patch]);
	useEffect(() => {
		setSelectedLines(null);
		setTextSelection(null);
		viewRef.current?.clearSelectedLines();
	}, [path, patch]);
	useEffect(() => {
		if (!onAddToChat) return;
		const update = () =>
			setTextSelection(
				containerRef.current ? readPatchTextSelection(containerRef.current) : null,
			);
		document.addEventListener("selectionchange", update);
		return () => document.removeEventListener("selectionchange", update);
	}, [onAddToChat]);
	useEffect(
		() => () => {
			if (resetFrameRef.current !== null) cancelAnimationFrame(resetFrameRef.current);
		},
		[],
	);
	const clearSelection = useCallback(() => {
		setTextSelection(null);
		setSelectedLines(null);
		viewRef.current?.clearSelectedLines();
	}, []);
	const addToChat = useCallback(
		(range: SelectedLineRange) => {
			const requests = patchSelectionRequests(path, fileDiff, range);
			if (!onAddToChat || requests.length === 0) return;
			window.getSelection()?.removeAllRanges();
			clearSelection();
			// The gutter commits its selection after its callback returns.
			if (resetFrameRef.current !== null) cancelAnimationFrame(resetFrameRef.current);
			resetFrameRef.current = requestAnimationFrame(() => {
				resetFrameRef.current = null;
				clearSelection();
			});
			onAddToChat(requests);
		},
		[path, fileDiff, onAddToChat, clearSelection],
	);
	const activeRange = textSelection ?? selectedLines?.range;
	const items = useMemo(
		() => [
			{
				id: ITEM_ID,
				type: "diff" as const,
				fileDiff,
				version: patchHash,
			},
		],
		[fileDiff, patchHash],
	);

	return (
		<div
			ref={containerRef}
			className={`relative min-h-0 min-w-0 flex-1 overflow-hidden bg-background ${className ?? ""}`}
			data-turn-review-diff={path}
			onPointerUp={() => {
				if (onAddToChat && containerRef.current) {
					setTextSelection(readPatchTextSelection(containerRef.current));
				}
			}}
			onKeyDown={(event) => {
				if (event.key === "Escape" && activeRange) {
					event.stopPropagation();
					window.getSelection()?.removeAllRanges();
					clearSelection();
				}
			}}
		>
			<CodeView
				ref={viewRef}
				className="h-full min-h-0 min-w-0 overflow-x-hidden overflow-y-auto"
				items={items}
				disableWorkerPool
				selectedLines={selectedLines}
				onSelectedLinesChange={setSelectedLines}
				options={{
					...workspaceDiffViewOptions(theme, true),
					unsafeCSS: codeReviewControlsCss,
					enableLineSelection: Boolean(onAddToChat),
					enableGutterUtility: Boolean(onAddToChat),
					onGutterUtilityClick: onAddToChat ? addToChat : undefined,
				}}
			/>
			{activeRange && onAddToChat && (
				<button
					type="button"
					className="dcc-snippet-trigger absolute right-3 top-3 z-20"
					onMouseDown={(event) => event.preventDefault()}
					onClick={() => addToChat(activeRange)}
				>
					<MessageSquarePlus className="size-3.5" aria-hidden />
					{t("turnReview.timeline.addToChat")}
				</button>
			)}
		</div>
	);
}
