import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import {
	$getNodeByKey,
	HISTORY_PUSH_TAG,
	SKIP_DOM_SELECTION_TAG,
} from "lexical";
import {
	Braces,
	ChevronDown,
	Eye,
	FileText,
	ImageIcon,
	Layers3,
	Loader2,
	X,
} from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { pathBasename } from "@/lib/path-basename";
import {
	$readComposerContext,
	type ComposerContextItem,
} from "./editor/composer-context";
import "./composer-context.css";

type Preview = {
	kind: "image" | "text" | "unavailable";
	content: string | null;
	dataUrl: string | null;
	reason: string | null;
};
const icons = { file: FileText, image: ImageIcon, snippet: Braces };

export function ComposerContextReview({
	workspaceRoot,
	draftKey,
	disabled = false,
}: {
	workspaceRoot: string | null;
	draftKey: string;
	disabled?: boolean;
}) {
	const [editor] = useLexicalComposerContext();
	const { t } = useTranslation("common");
	const [items, setItems] = useState<ComposerContextItem[]>([]);
	const [expanded, setExpanded] = useState(false);
	const [previewKey, setPreviewKey] = useState<string | null>(null);
	const summary = useRef<HTMLButtonElement>(null);
	const regionId = useId();
	useEffect(() => {
		const sync = () => {
			const next = editor.getEditorState().read($readComposerContext);
			setItems((current) =>
				current.length === next.length &&
				current.every(
					(item, i) =>
						item.key === next[i]?.key &&
						item.kind === next[i]?.kind &&
						item.value === next[i]?.value,
				)
					? current
					: next,
			);
		};
		sync();
		return editor.registerUpdateListener(({ dirtyElements, dirtyLeaves }) => {
			if (dirtyElements.size || dirtyLeaves.size) sync();
		});
	}, [editor]);
	useEffect(() => {
		setExpanded(false);
		setPreviewKey(null);
	}, [draftKey]);
	const label = (item: ComposerContextItem) =>
		item.kind === "snippet"
			? t("composer.context.snippet")
			: pathBasename(item.value);
	const remove = (item: ComposerContextItem) => {
		if (disabled || !editor.isEditable()) return;
		editor.update(
			() => {
				// The key identifies this occurrence, even when the same file appears twice.
				if ($readComposerContext().some((current) => current.key === item.key))
					$getNodeByKey(item.key)?.remove();
			},
			{ tag: [HISTORY_PUSH_TAG, SKIP_DOM_SELECTION_TAG] },
		);
		requestAnimationFrame(() => {
			if (summary.current) summary.current.focus();
			else editor.focus();
		});
	};
	const preview = items.find((item) => item.key === previewKey) ?? null;
	if (!items.length) return null;
	return (
		<div className="composer-context-review">
			<button
				type="button"
				ref={summary}
				className="composer-context-summary"
				aria-expanded={expanded}
				aria-controls={regionId}
				onClick={() => setExpanded((value) => !value)}
			>
				<span className="composer-context-mark">
					<Layers3 size={14} aria-hidden />
				</span>
				<span>{t("composer.context.title")}</span>
				<span className="composer-context-count">{items.length}</span>
				<ChevronDown
					size={14}
					className={expanded ? "rotate-180" : ""}
					aria-hidden
				/>
			</button>
			{expanded && (
				<div id={regionId} className="composer-context-body">
					<p className="composer-context-description">
						{t("composer.context.description")}
					</p>
					<div className="composer-context-grid">
						{items.map((item) => {
							const Icon = icons[item.kind];
							return (
								<div
									className="composer-context-item"
									data-kind={item.kind}
									key={item.key}
								>
									<Icon
										size={16}
										className="composer-context-icon"
										aria-hidden
									/>
									<button
										type="button"
										className="composer-context-item-content"
										onClick={() => setPreviewKey(item.key)}
										aria-label={t("composer.context.previewItem", {
											name: item.kind === "snippet" ? label(item) : item.value,
										})}
									>
										<span className="composer-context-item-copy">
											<strong>{label(item)}</strong>
											<small
												title={item.kind === "snippet" ? undefined : item.value}
											>
												{item.kind === "snippet"
													? t("composer.context.characters", {
															count: item.value.length,
														})
													: item.value}
											</small>
										</span>
										<Eye
											size={14}
											className="shrink-0 text-muted-foreground"
											aria-hidden
										/>
									</button>
									<button
										type="button"
										className="composer-context-action"
										disabled={disabled}
										onClick={() => remove(item)}
										aria-label={t("composer.context.removeItem", {
											name: item.kind === "snippet" ? label(item) : item.value,
										})}
									>
										<X size={14} aria-hidden />
									</button>
								</div>
							);
						})}
					</div>
				</div>
			)}
			<Dialog
				open={Boolean(preview)}
				onOpenChange={(open) => {
					if (!open) setPreviewKey(null);
				}}
			>
				<DialogContent className="composer-context-dialog sm:max-w-2xl">
					<DialogHeader>
						<DialogTitle>
							{preview ? label(preview) : t("composer.context.title")}
						</DialogTitle>
						<DialogDescription className="break-all">
							{preview?.kind === "snippet"
								? t("composer.context.snippetDescription")
								: preview?.value}
						</DialogDescription>
					</DialogHeader>
					{preview && (
						<ContextPreview
							key={`${draftKey}:${preview.key}`}
							item={preview}
							workspaceRoot={workspaceRoot}
						/>
					)}
				</DialogContent>
			</Dialog>
		</div>
	);
}

function ContextPreview({
	item,
	workspaceRoot,
}: {
	item: ComposerContextItem;
	workspaceRoot: string | null;
}) {
	const { t } = useTranslation("common");
	const [result, setResult] = useState<Preview | null>(
		item.kind === "snippet"
			? { kind: "text", content: item.value, dataUrl: null, reason: null }
			: null,
	);
	const [failed, setFailed] = useState(false);
	const [attempt, setAttempt] = useState(0);
	useEffect(() => {
		if (item.kind === "snippet") return;
		let active = true;
		setResult(null);
		setFailed(false);
		void invoke<Preview>("preview_composer_attachment", {
			workspaceRoot,
			filePath: item.value,
		})
			.then((value) => {
				if (active) setResult(value);
			})
			.catch(() => {
				if (active) setFailed(true);
			});
		return () => {
			active = false;
		};
	}, [item.kind, item.value, workspaceRoot, attempt]);
	if (failed)
		return (
			<div className="composer-context-placeholder" role="status">
				<p>{t("composer.context.failed")}</p>
				<Button
					variant="outline"
					size="sm"
					onClick={() => setAttempt((value) => value + 1)}
				>
					{t("composer.context.retry")}
				</Button>
			</div>
		);
	if (!result)
		return (
			<div className="composer-context-placeholder" role="status">
				<Loader2 className="size-5 animate-spin" />
				<span>{t("composer.context.loading")}</span>
			</div>
		);
	if (result.kind === "unavailable")
		return (
			<p className="composer-context-placeholder">
				{t(
					result.reason === "tooLarge"
						? "composer.context.tooLarge"
						: "composer.context.binary",
				)}
			</p>
		);
	return result.kind === "image" ? (
		<img
			className="composer-context-image"
			src={result.dataUrl ?? undefined}
			alt={pathBasename(item.value)}
			onError={() => setFailed(true)}
		/>
	) : (
		<pre className="composer-context-text" tabIndex={0}>
			{result.content}
		</pre>
	);
}
