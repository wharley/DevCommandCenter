import { Code2, X, Send, SquarePen, ListPlus, Plus } from "lucide-react";
import "./code-annotation.css";
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
	const [position, setPosition] = useState<{
		top: number;
		left: number;
	} | null>(null);

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
				data-dcc-browser-occluder="true"
				onMouseDown={onCancel}
				aria-hidden
			/>
			<div
				ref={cardRef}
				data-dcc-browser-occluder="true"
				role="dialog"
				aria-modal="true"
				aria-label={t("diffAnnotate.dialogLabel")}
				className="dcc-code-annotation fixed z-[71] flex max-h-[calc(100vh-1.5rem)] w-[396px] max-w-[calc(100vw-1.5rem)] flex-col overflow-hidden"
				style={{
					top: position?.top ?? anchor.top,
					left: position?.left ?? anchor.left,
					visibility: position ? "visible" : "hidden",
				}}
				onMouseDown={(event) => event.stopPropagation()}
				onKeyDown={(event) => {
					if (event.key === "Escape") {
						event.preventDefault();
						event.stopPropagation();
						onCancel();
					}
					if (event.key === "Tab") {
						const controls = [
							...event.currentTarget.querySelectorAll<HTMLElement>(
								'button:not(:disabled), textarea, [tabindex="0"]',
							),
						].filter((el) => el.getClientRects().length > 0);
						const first = controls[0];
						const last = controls.at(-1);
						if (event.shiftKey && document.activeElement === first) {
							event.preventDefault();
							last?.focus();
						} else if (!event.shiftKey && document.activeElement === last) {
							event.preventDefault();
							first?.focus();
						}
					}
				}}
			>
				<div className="dcc-code-annotation-header">
					<span className="dcc-code-annotation-mark">
						<Code2 size={16} aria-hidden />
					</span>
					<div className="min-w-0 flex-1">
						<h2 className="dcc-code-annotation-title">
							{t("diffAnnotate.dialogLabel")}
						</h2>
						<p className="dcc-code-annotation-path">{request.path}</p>
					</div>
					<Button
						type="button"
						variant="ghost"
						size="icon-xs"
						aria-label={t("diffAnnotate.close")}
						onClick={onCancel}
					>
						<X size={14} />
					</Button>
				</div>
				<div className="dcc-code-annotation-context">
					<span className="font-mono tabular-nums">{lineLabel}</span>
					{sideLabel && <span data-side="original">{sideLabel}</span>}
				</div>
				<div className="dcc-code-annotation-body">
					{request.snippet && (
						<div className="dcc-code-annotation-snippet">
							<pre tabIndex={0} aria-label={t("diffAnnotate.selectedCode")}>
								<code>{request.snippet.slice(0, 6000)}</code>
							</pre>
							{request.snippet.length > 6000 && (
								<small>{t("diffAnnotate.previewLimited")}</small>
							)}
						</div>
					)}

					<Textarea
						ref={textareaRef}
						aria-label={t("diffAnnotate.instructionPlaceholder")}
						value={instruction}
						onChange={(event) => setInstruction(event.target.value)}
						placeholder={t("diffAnnotate.instructionPlaceholder")}
						className="dcc-code-annotation-input"
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
								!event.nativeEvent.isComposing &&
								canSubmit
							) {
								event.preventDefault();
								onSubmit(trimmed, false);
							}
						}}
					/>

					<div className="dcc-code-annotation-hints">
						<span className="inline-flex items-center gap-1">
							<InlineShortcutDisplay keys={["⌘", "↵"]} />
							<Send className="mr-1 size-3" aria-hidden />
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
								<ListPlus className="mr-1 size-3" aria-hidden />
								{t("diffAnnotate.addToReview")}
							</Button>
						) : null}
					</div>
				</div>

				<div className="dcc-code-annotation-footer">
					{canEditInComposer ? (
						<Button
							type="button"
							variant="ghost"
							size="xs"
							className="mr-auto h-7 px-2 text-muted-foreground hover:text-foreground"
							onClick={() => onEditInComposer(trimmed)}
						>
							<SquarePen className="mr-1 size-3" aria-hidden />
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
						<Plus className="mr-1 size-3" aria-hidden />
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
						<Send className="mr-1 size-3" aria-hidden />
						{t("diffAnnotate.send")}
					</Button>
				</div>
			</div>
		</>,
		document.body,
	);
}
