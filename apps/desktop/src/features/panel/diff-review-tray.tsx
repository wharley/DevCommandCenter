import { ChevronDown, ChevronUp, X } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import type { DiffAnnotationRequest } from "@/features/editor/WorkspaceEditorSurface";

/** A diff snippet collected into the multi-snippet review buffer. */
export type ReviewAnnotation = {
	id: string;
	request: DiffAnnotationRequest;
	note: string;
};

function lineLabel(request: DiffAnnotationRequest): string {
	return request.startLine === request.endLine
		? `L${request.startLine}`
		: `L${request.startLine}–${request.endLine}`;
}

/**
 * Floating tray listing the snippets gathered during a review. Rendered above
 * both the diff surface and the chat so collection survives navigation; sending
 * composes every snippet (and its note) into a single agent turn.
 */
export function DiffReviewTray({
	annotations,
	onRemove,
	onClear,
	onSubmit,
}: {
	annotations: ReviewAnnotation[];
	onRemove: (id: string) => void;
	onClear: () => void;
	onSubmit: (input: { instruction: string; newSession: boolean }) => void;
}) {
	const { t } = useTranslation("common");
	const [expanded, setExpanded] = useState(true);
	const [instruction, setInstruction] = useState("");

	if (annotations.length === 0) {
		return null;
	}

	return (
		<div className="fixed left-1/2 top-14 z-40 w-[380px] max-w-[calc(100vw-2rem)] -translate-x-1/2 rounded-xl border border-border bg-popover text-popover-foreground shadow-lg">
			<div className="flex items-center justify-between gap-2 border-b border-border/60 px-3 py-2">
				<button
					type="button"
					onClick={() => setExpanded((value) => !value)}
					aria-label={
						expanded
							? t("diffAnnotate.review.collapse")
							: t("diffAnnotate.review.expand")
					}
					className="flex min-w-0 items-center gap-1.5 text-[12px] font-medium"
				>
					{expanded ? (
						<ChevronDown className="size-3.5 shrink-0" />
					) : (
						<ChevronUp className="size-3.5 shrink-0" />
					)}
					<span>{t("diffAnnotate.review.title")}</span>
					<span className="shrink-0 rounded bg-muted px-1.5 py-0.5 text-[10px] tabular-nums text-muted-foreground">
						{t("diffAnnotate.review.items", { count: annotations.length })}
					</span>
				</button>
				<Button
					type="button"
					variant="ghost"
					size="xs"
					className="text-muted-foreground"
					onClick={onClear}
				>
					{t("diffAnnotate.review.clear")}
				</Button>
			</div>
			{expanded ? (
				<div className="p-3">
					<ul className="mb-2.5 max-h-40 space-y-1.5 overflow-y-auto">
						{annotations.map((annotation) => (
							<li
								key={annotation.id}
								className="flex items-start gap-2 rounded-md border border-border/50 bg-background/60 px-2 py-1.5"
							>
								<div className="min-w-0 flex-1">
									<div className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
										<span
											className="truncate font-mono"
											title={annotation.request.path}
										>
											{annotation.request.path}
										</span>
										<span className="shrink-0 rounded bg-muted px-1 py-0.5 tabular-nums">
											{lineLabel(annotation.request)}
										</span>
										{annotation.request.side === "original" ? (
											<span className="shrink-0 rounded bg-destructive/15 px-1 py-0.5 text-destructive">
												{t("diffAnnotate.deletedSide")}
											</span>
										) : null}
									</div>
									{annotation.note.trim().length > 0 ? (
										<p className="mt-0.5 truncate text-[12px] text-foreground">
											{annotation.note.trim()}
										</p>
									) : null}
								</div>
								<button
									type="button"
									onClick={() => onRemove(annotation.id)}
									aria-label={t("diffAnnotate.review.remove")}
									className="shrink-0 rounded p-0.5 text-muted-foreground hover:text-foreground"
								>
									<X className="size-3.5" />
								</button>
							</li>
						))}
					</ul>
					<Textarea
						value={instruction}
						onChange={(event) => setInstruction(event.target.value)}
						placeholder={t("diffAnnotate.review.overallPlaceholder")}
						className="min-h-[56px] text-[13px]"
					/>
					<div className="mt-2.5 flex items-center justify-end gap-1.5">
						<Button
							type="button"
							variant="outline"
							size="xs"
							onClick={() =>
								onSubmit({ instruction: instruction.trim(), newSession: true })
							}
						>
							{t("diffAnnotate.review.newSession")}
						</Button>
						<Button
							type="button"
							variant="default"
							size="xs"
							onClick={() =>
								onSubmit({ instruction: instruction.trim(), newSession: false })
							}
						>
							{t("diffAnnotate.review.send")}
						</Button>
					</div>
				</div>
			) : null}
		</div>
	);
}
