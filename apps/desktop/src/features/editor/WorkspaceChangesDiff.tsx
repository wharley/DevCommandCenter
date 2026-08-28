import {
	parseDiffFromFile,
	type DiffLineAnnotation,
	type SelectedLineRange,
} from "@pierre/diffs";
import {
	CodeView,
	type CodeViewHandle,
} from "@pierre/diffs/react";
import { MessageSquare } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useAppearance } from "@/components/theme-provider";
import type {
	DiffAnnotationPayload,
	DiffMachineAnnotation,
} from "./diff-types";
import {
	annotationPayloadFromPierreRange,
	groupWorkspaceDiffAnnotations,
	workspaceDiffAnnotationCss,
	workspaceDiffContentHash,
} from "./workspace-changes-diff.logic";
import { workspaceDiffViewOptions } from "./workspace-diff-view-options";

export type WorkspaceChangesDiffProps = {
	path: string;
	originalText: string;
	modifiedText: string;
	inline: boolean;
	focusLine?: number | null;
	machineAnnotations?: DiffMachineAnnotation[];
	onAnnotate?: (payload: DiffAnnotationPayload) => void;
	annotateLabel?: string;
	onMachineAnnotationClick?: (input: {
		annotation: DiffMachineAnnotation;
		anchor: { top: number; left: number };
	}) => void;
	reviewCommentLabel?: string;
	className?: string;
};

type AnnotationMetadata = {
	annotations: DiffMachineAnnotation[];
};

type CodeViewLineSelection = {
	id: string;
	range: SelectedLineRange;
};

const ITEM_ID = "dcc-workspace-changes";

function triggerAnchor(target: HTMLElement | null): { top: number; left: number } {
	const rect = target?.getBoundingClientRect();
	return {
		top: rect?.bottom ?? Math.round(window.innerHeight / 2),
		left: rect?.left ?? Math.round(window.innerWidth / 2),
	};
}

function AnnotationCallout({
	annotation,
	reviewCommentLabel,
	onClick,
}: {
	annotation: DiffMachineAnnotation;
	reviewCommentLabel: string;
	onClick?: WorkspaceChangesDiffProps["onMachineAnnotationClick"];
}) {
	const isReview = annotation.source === "forge-review";
	const content = (
		<>
			<MessageSquare className="size-3 shrink-0" aria-hidden />
			<span className="truncate">{isReview ? reviewCommentLabel : annotation.title}</span>
		</>
	);
	const className =
		"my-1 inline-flex max-w-full items-center gap-1.5 rounded-md border border-border/70 bg-muted/70 px-2 py-1 text-[10px] text-foreground shadow-sm";

	if (isReview && onClick) {
		return (
			<button
				type="button"
				className={`${className} hover:bg-muted`}
				title={annotation.title}
				onClick={(event) =>
					onClick({
						annotation,
						anchor: triggerAnchor(event.currentTarget),
					})
				}
			>
				{content}
			</button>
		);
	}

	return (
		<div className={className} title={annotation.title}>
			{content}
		</div>
	);
}

export default function WorkspaceChangesDiff({
	path,
	originalText,
	modifiedText,
	inline,
	focusLine,
	machineAnnotations = [],
	onAnnotate,
	annotateLabel = "Annotate selection",
	onMachineAnnotationClick,
	reviewCommentLabel = "Review comment",
	className,
}: WorkspaceChangesDiffProps) {
	const { theme } = useAppearance();
	const viewRef = useRef<CodeViewHandle<AnnotationMetadata> | null>(null);
	const containerRef = useRef<HTMLDivElement | null>(null);
	const selectionButtonRef = useRef<HTMLButtonElement | null>(null);
	const [selectedLines, setSelectedLines] =
		useState<CodeViewLineSelection | null>(null);
	const originalHash = useMemo(
		() => workspaceDiffContentHash(originalText),
		[originalText],
	);
	const modifiedHash = useMemo(
		() => workspaceDiffContentHash(modifiedText),
		[modifiedText],
	);
	const fileDiff = useMemo(
		() =>
			parseDiffFromFile(
				{ name: path, contents: originalText, cacheKey: `${path}:${originalHash}` },
				{ name: path, contents: modifiedText, cacheKey: `${path}:${modifiedHash}` },
			),
		[modifiedHash, modifiedText, originalHash, originalText, path],
	);
	const annotations = useMemo<DiffLineAnnotation<AnnotationMetadata>[]>(
		() =>
			groupWorkspaceDiffAnnotations(machineAnnotations).map((group) => ({
				side: group.side,
				lineNumber: group.lineNumber,
				metadata: { annotations: group.annotations },
			})),
		[machineAnnotations],
	);
	const annotationCss = useMemo(
		() => workspaceDiffAnnotationCss(machineAnnotations),
		[machineAnnotations],
	);
	const items = useMemo(
		() => [
			{
				id: ITEM_ID,
				type: "diff" as const,
				fileDiff,
				annotations,
				version: (originalHash ^ modifiedHash) >>> 0,
			},
		],
		[annotations, fileDiff, modifiedHash, originalHash],
	);

	useEffect(() => {
		if (!focusLine) return;
		const frame = requestAnimationFrame(() => {
			viewRef.current?.scrollTo({
				type: "line",
				id: ITEM_ID,
				lineNumber: focusLine,
				side: "additions",
				align: "center",
			});
		});
		return () => cancelAnimationFrame(frame);
	}, [focusLine, modifiedHash, originalHash]);

	const handleAnnotate = useCallback(
		(range: CodeViewLineSelection["range"], trigger?: HTMLElement | null) => {
			if (!onAnnotate) return;
			onAnnotate(
				annotationPayloadFromPierreRange({
					range,
					originalText,
					modifiedText,
					anchor: triggerAnchor(trigger ?? containerRef.current),
				}),
			);
			setSelectedLines(null);
		},
		[modifiedText, onAnnotate, originalText],
	);

	return (
		<div
			ref={containerRef}
			className={`relative min-h-0 min-w-0 flex-1 overflow-hidden bg-background ${className ?? ""}`}
		>
			<CodeView<AnnotationMetadata>
				ref={viewRef}
				className="h-full min-h-0 min-w-0 overflow-x-hidden overflow-y-auto"
				items={items}
				selectedLines={selectedLines}
				onSelectedLinesChange={setSelectedLines}
				disableWorkerPool
				options={{
					...workspaceDiffViewOptions<AnnotationMetadata>(theme, inline),
					unsafeCSS: annotationCss,
					enableGutterUtility: Boolean(onAnnotate),
					enableLineSelection: Boolean(onAnnotate),
					onGutterUtilityClick: (range) => handleAnnotate(range),
				}}
				renderAnnotation={(annotation) => (
					<div className="flex min-w-0 flex-col px-2 py-0.5">
						{annotation.metadata?.annotations.map((entry, index) => (
							<AnnotationCallout
								key={`${entry.source}:${entry.id ?? entry.title}:${index}`}
								annotation={entry}
								reviewCommentLabel={reviewCommentLabel}
								onClick={onMachineAnnotationClick}
							/>
						))}
					</div>
				)}
			/>
			{selectedLines && onAnnotate ? (
				<button
					ref={selectionButtonRef}
					type="button"
					className="absolute right-3 top-3 z-20 inline-flex items-center gap-1.5 rounded-md border border-primary/30 bg-primary px-2.5 py-1.5 text-[11px] font-medium text-primary-foreground shadow-lg"
					onClick={(event) =>
						handleAnnotate(selectedLines.range, event.currentTarget)
					}
					aria-label={annotateLabel}
					title={annotateLabel}
				>
					<MessageSquare className="size-3.5" aria-hidden />
					{annotateLabel}
				</button>
			) : null}
		</div>
	);
}
