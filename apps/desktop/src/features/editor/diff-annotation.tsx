import { useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { InlineShortcutDisplay } from "@/features/shortcuts/InlineShortcutDisplay";
import type { DiffAnnotationPayload } from "./diff-types";

/** A diff/file annotation bound to the file it was selected in (anchor stripped). */
export type DiffAnnotationRequest = Omit<DiffAnnotationPayload, "anchor"> & {
	path: string;
};

export type DiffAnnotationSubmit = {
	request: DiffAnnotationRequest;
	instruction: string;
	newSession: boolean;
};

export type PendingAnnotation = {
	request: DiffAnnotationRequest;
	anchor: { top: number; left: number };
};

/**
 * Shared overlay for turning a selected snippet into an agent instruction. Used by
 * both the diff surface and the whole-file surface — the popover is agnostic about
 * where the selection came from, it only needs the request + anchor.
 */
export function DiffAnnotationPopover({
	pending,
	canEditInComposer,
	canAddToReview,
	onSubmit,
	onEditInComposer,
	onAddToReview,
	onCancel,
}: {
	pending: PendingAnnotation;
	canEditInComposer: boolean;
	canAddToReview: boolean;
	onSubmit: (instruction: string, newSession: boolean) => void;
	onEditInComposer: (instruction: string) => void;
	onAddToReview: (note: string) => void;
	onCancel: () => void;
}) {
	const { t } = useTranslation("common");
	const [instruction, setInstruction] = useState("");
	const textareaRef = useRef<HTMLTextAreaElement | null>(null);
	const cardRef = useRef<HTMLDivElement | null>(null);
	// Resolved viewport position. Kept null until measured so the card never
	// flashes at an unclamped spot (which is what pushed the actions off-screen).
	const [position, setPosition] = useState<{ top: number; left: number } | null>(
		null,
	);

	const { request, anchor } = pending;

	// Measure the rendered card and place it fully inside the viewport: prefer
	// above the trigger, fall back to below, then clamp on every edge. This
	// guarantees the footer (and the primary "Send" action) is always visible.
	useLayoutEffect(() => {
		const card = cardRef.current;
		if (!card) {
			return;
		}

		const measure = () => {
			const margin = 12;
			const gap = 8;
			const { offsetWidth: width, offsetHeight: height } = card;
			const viewportW = window.innerWidth;
			const viewportH = window.innerHeight;

			let left = Math.min(anchor.left, viewportW - width - margin);
			left = Math.max(margin, left);

			const above = anchor.top - gap - height;
			const below = anchor.top + 24;
			let top = above >= margin ? above : below;
			top = Math.min(top, viewportH - height - margin);
			top = Math.max(margin, top);

			setPosition({ top, left });
		};

		measure();
		window.addEventListener("resize", measure);
		return () => window.removeEventListener("resize", measure);
	}, [anchor.left, anchor.top]);

	const isPositioned = position !== null;

	// The card is hidden until its viewport-safe position has been measured. Trying
	// to focus the textarea during the first mount is therefore ignored by some
	// browsers/webviews. Focus it only after the card is visible and keep this modal
	// focus scope from being immediately stolen back by the underlying editor.
	useLayoutEffect(() => {
		if (!isPositioned) {
			return;
		}

		const card = cardRef.current;
		const textarea = textareaRef.current;
		if (!card || !textarea) {
			return;
		}

		const focusInstruction = () => {
			textarea.focus({ preventScroll: true });
		};
		const keepFocusInDialog = (event: FocusEvent) => {
			if (!(event.target instanceof Node) || !card.contains(event.target)) {
				focusInstruction();
			}
		};

		document.addEventListener("focusin", keepFocusInDialog);
		focusInstruction();
		const focusFrame = requestAnimationFrame(() => {
			if (!card.contains(document.activeElement)) {
				focusInstruction();
			}
		});

		return () => {
			document.removeEventListener("focusin", keepFocusInDialog);
			cancelAnimationFrame(focusFrame);
		};
	}, [isPositioned]);

	const lineLabel =
		request.startLine === request.endLine
			? `L${request.startLine}`
			: `L${request.startLine}–${request.endLine}`;
	const sideLabel =
		request.side === "original" ? t("diffAnnotate.deletedSide") : null;
	const trimmed = instruction.trim();
	const canSubmit = trimmed.length > 0;

	return createPortal(
		<>
			<div
				className="fixed inset-0 z-[70] bg-black/5 supports-backdrop-filter:backdrop-blur-[1px] animate-in fade-in-0 duration-100"
				onMouseDown={onCancel}
				aria-hidden
			/>
			<div
				ref={cardRef}
				role="dialog"
				aria-modal="true"
				aria-label={t("diffAnnotate.dialogLabel")}
				className="fixed z-[71] flex max-h-[calc(100vh-1.5rem)] w-[348px] max-w-[calc(100vw-1.5rem)] origin-top flex-col overflow-hidden rounded-xl border border-border/80 bg-popover text-popover-foreground shadow-xl ring-1 ring-foreground/10 animate-in fade-in-0 zoom-in-95 duration-100"
				style={{
					top: position?.top ?? anchor.top,
					left: position?.left ?? anchor.left,
					visibility: position ? "visible" : "hidden",
				}}
				onMouseDown={(event) => event.stopPropagation()}
			>
				<div className="flex items-center gap-1.5 border-b border-border/60 bg-muted/30 px-3 py-2 text-[11px]">
					<span className="size-1.5 shrink-0 rounded-full bg-primary/70" aria-hidden />
					<span
						className="min-w-0 flex-1 truncate font-mono text-muted-foreground"
						title={request.path}
						dir="rtl"
					>
						{request.path}
					</span>
					<span className="shrink-0 rounded bg-muted px-1.5 py-0.5 font-mono font-medium tabular-nums text-foreground/80">
						{lineLabel}
					</span>
					{sideLabel ? (
						<span className="shrink-0 rounded bg-destructive/15 px-1.5 py-0.5 font-medium text-destructive">
							{sideLabel}
						</span>
					) : null}
				</div>

				<div className="flex min-h-0 flex-col gap-2.5 overflow-y-auto p-3">
					<Textarea
						ref={textareaRef}
						value={instruction}
						onChange={(event) => setInstruction(event.target.value)}
						placeholder={t("diffAnnotate.instructionPlaceholder")}
						className="min-h-[76px] resize-none text-[13px] leading-relaxed"
						onKeyDown={(event) => {
							if (event.key === "Escape") {
								event.preventDefault();
								event.stopPropagation();
								onCancel();
								return;
							}
							if (
								(event.metaKey || event.ctrlKey) &&
								event.key === "Enter" &&
								canSubmit
							) {
								event.preventDefault();
								onSubmit(trimmed, false);
							}
						}}
					/>

					<div className="flex items-center justify-between gap-2 text-[10.5px] text-muted-foreground">
						<span className="inline-flex items-center gap-1">
							<InlineShortcutDisplay keys={["⌘", "↵"]} />
							{t("diffAnnotate.send")}
						</span>
						{canAddToReview ? (
							<Button
								type="button"
								variant="ghost"
								size="xs"
								className="h-6 px-1.5 text-muted-foreground hover:text-foreground"
								disabled={!canSubmit}
								onClick={() => onAddToReview(trimmed)}
							>
								{t("diffAnnotate.addToReview")}
							</Button>
						) : null}
					</div>
				</div>

				<div className="flex flex-wrap items-center justify-end gap-1.5 border-t border-border/60 bg-muted/20 px-3 py-2.5">
					{canEditInComposer ? (
						<Button
							type="button"
							variant="ghost"
							size="xs"
							className="mr-auto h-7 px-2 text-muted-foreground hover:text-foreground"
							onClick={() => onEditInComposer(trimmed)}
						>
							{t("diffAnnotate.editInComposer")}
						</Button>
					) : null}
					<Button
						type="button"
						variant="outline"
						size="xs"
						className="h-7"
						disabled={!canSubmit}
						onClick={() => onSubmit(trimmed, true)}
					>
						{t("diffAnnotate.newSession")}
					</Button>
					<Button
						type="button"
						variant="default"
						size="xs"
						className="h-7 shadow-sm"
						disabled={!canSubmit}
						onClick={() => onSubmit(trimmed, false)}
					>
						{t("diffAnnotate.send")}
					</Button>
				</div>
			</div>
		</>,
		document.body,
	);
}
