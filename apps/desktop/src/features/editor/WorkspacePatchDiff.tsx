import codeReviewControlsCss from "./code-review-controls.css?inline";
import "./code-review-controls.css";
import type { DiffLineAnnotation, SelectedLineRange } from "@pierre/diffs";
import { CodeView, type CodeViewHandle } from "@pierre/diffs/react";
import { MessageSquarePlus } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useAppearance } from "@/components/theme-provider";
import {
	groupWorkspaceDiffAnnotations,
	workspaceDiffAnnotationCss,
	workspaceDiffContentHash,
} from "./workspace-changes-diff.logic";
import { AnnotationCallout } from "./WorkspaceChangesDiff";
import type { DiffMachineAnnotation } from "./diff-types";
import { workspaceDiffViewOptions } from "./workspace-diff-view-options";
import {
	parseWorkspacePatch,
	patchNewSideLines,
	patchSelectionRequests,
} from "./workspace-patch-diff.logic";
import type { DiffAnnotationRequest } from "./diff-annotation";
import { readPatchTextSelection } from "./patch-text-selection";

export type WorkspacePatchDiffProps = {
	path: string;
	patch: string;
	className?: string;
	onAddToChat?: (requests: DiffAnnotationRequest[]) => void;
	/** Markings on new-side lines; ones outside the patch's hunks are dropped. */
	machineAnnotations?: DiffMachineAnnotation[];
	onMachineAnnotationClick?: (input: {
		annotation: DiffMachineAnnotation;
		/** The marked lines as the patch shows them. */
		snippet: string;
	}) => void;
};

type AnnotationMetadata = { annotations: DiffMachineAnnotation[] };
const NO_ANNOTATIONS: DiffMachineAnnotation[] = [];

const ITEM_ID = "dcc-workspace-patch";

/** Renders one captured Git patch with the same read-only visual system as the dock. */
export default function WorkspacePatchDiff({
	path,
	patch,
	className,
	onAddToChat,
	machineAnnotations = NO_ANNOTATIONS,
	onMachineAnnotationClick,
}: WorkspacePatchDiffProps) {
	const { t } = useTranslation("common");
	const { theme } = useAppearance();
	const containerRef = useRef<HTMLDivElement>(null);
	const viewRef = useRef<CodeViewHandle<AnnotationMetadata>>(null);
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
	const newSideLines = useMemo(() => patchNewSideLines(patch), [patch]);
	const visibleAnnotations = useMemo(
		() =>
			machineAnnotations.filter(
				(annotation) => annotation.side === "modified" && newSideLines.has(annotation.endLine),
			),
		[machineAnnotations, newSideLines],
	);
	const annotations = useMemo<DiffLineAnnotation<AnnotationMetadata>[]>(
		() =>
			groupWorkspaceDiffAnnotations(visibleAnnotations).map((group) => ({
				side: group.side,
				lineNumber: group.lineNumber,
				metadata: { annotations: group.annotations },
			})),
		[visibleAnnotations],
	);
	const annotationCss = useMemo(
		() => workspaceDiffAnnotationCss(visibleAnnotations),
		[visibleAnnotations],
	);
	const items = useMemo(
		() => [
			{
				id: ITEM_ID,
				type: "diff" as const,
				fileDiff,
				annotations,
				version: patchHash,
			},
		],
		[annotations, fileDiff, patchHash],
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
			<CodeView<AnnotationMetadata>
				ref={viewRef}
				className="h-full min-h-0 min-w-0 overflow-x-auto overflow-y-auto"
				items={items}
				disableWorkerPool
				selectedLines={selectedLines}
				onSelectedLinesChange={setSelectedLines}
				options={{
					...workspaceDiffViewOptions<AnnotationMetadata>(theme, true),
					unsafeCSS: `${codeReviewControlsCss}\n${annotationCss}`,
					enableLineSelection: Boolean(onAddToChat),
					enableGutterUtility: Boolean(onAddToChat),
					onGutterUtilityClick: onAddToChat ? addToChat : undefined,
				}}
				renderAnnotation={(annotation) => (
					<div className="flex min-w-0 flex-col px-2 py-0.5">
						{annotation.metadata?.annotations.map((entry, index) => (
							<AnnotationCallout
								key={`${entry.source}:${entry.title}:${index}`}
								annotation={entry}
								reviewCommentLabel=""
								onClick={
									onMachineAnnotationClick
										? ({ annotation: clicked }) => {
												const snippet: string[] = [];
												for (let line = clicked.startLine; line <= clicked.endLine; line += 1) {
													const text = newSideLines.get(line);
													if (text !== undefined) snippet.push(text);
												}
												onMachineAnnotationClick({
													annotation: clicked,
													snippet: snippet.join("\n"),
												});
											}
										: undefined
								}
							/>
						))}
					</div>
				)}
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
