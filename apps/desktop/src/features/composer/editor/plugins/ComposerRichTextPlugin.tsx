import { ListPlugin } from "@lexical/react/LexicalListPlugin";
import { MarkdownShortcutPlugin } from "@lexical/react/LexicalMarkdownShortcutPlugin";
import {
	COMPOSER_LIST_TRANSFORMERS,
	$isComposerSelectionInList,
} from "../composer-lists";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
	Bold,
	Italic,
	Strikethrough,
	Code,
	Link as LinkIcon,
	ExternalLink,
	Pencil,
	Unlink,
} from "lucide-react";
import { toast } from "sonner";
import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import { AutoLinkPlugin } from "@lexical/react/LexicalAutoLinkPlugin";
import { LinkPlugin } from "@lexical/react/LexicalLinkPlugin";
import {
	$createAutoLinkNode,
	$createLinkNode,
	$isAutoLinkNode,
	$isLinkNode,
	$toggleLink,
	type LinkNode,
} from "@lexical/link";
import {
	$getNearestNodeFromDOMNode,
	$getNodeByKey,
	$getRoot,
	$getSelection,
	$isRangeSelection,
	$createTextNode,
	$setSelection,
	COMMAND_PRIORITY_HIGH,
	FORMAT_TEXT_COMMAND,
	PASTE_COMMAND,
	type RangeSelection,
	type TextFormatType,
} from "lexical";
import { Button } from "@/components/ui/button";
import {
	Popover,
	PopoverAnchor,
	PopoverContent,
} from "@/components/ui/popover";
import { openExternal } from "@/lib/shell-api";
import { COMPOSER_LINK_MATCHERS, normalizeComposerUrl } from "../rich-text";
import { SiteLinkIcon } from "@/components/site-link-icon";
import { ComposerLinkIconsPlugin } from "./ComposerLinkIconsPlugin";
import "../../composer-rich-text.css";

const validateUrl = (url: string) => normalizeComposerUrl(url) !== null;

type LinkDraft = {
	key: string | null;
	text: string;
	url: string;
	mode: "menu" | "text" | "url";
	anchor: DOMRect | null;
};
const formats = [
	{ format: "bold", Icon: Bold, shortcut: "B" },
	{ format: "italic", Icon: Italic, shortcut: "I" },
	{ format: "strikethrough", Icon: Strikethrough, shortcut: null },
	{ format: "code", Icon: Code, shortcut: null },
] as const;
function $selectedLink(): LinkNode | null {
	const selection = $getSelection();
	if (!$isRangeSelection(selection)) return null;
	const node = selection.anchor.getNode();
	const parent = node.getParent();
	const link = $isLinkNode(node) ? node : $isLinkNode(parent) ? parent : null;
	return link && !($isAutoLinkNode(link) && link.getIsUnlinked()) ? link : null;
}

export function ComposerRichTextPlugin({
	disabled = false,
	draftKey,
}: {
	disabled?: boolean;
	draftKey: string;
}) {
	const [editor] = useLexicalComposerContext();
	const { t } = useTranslation("common");
	const label = (key: string) => t(`composer.richText.${key}`);
	const [draft, setDraft] = useState<LinkDraft | null>(null);
	const [error, setError] = useState(false);
	const [inList, setInList] = useState(false);
	const [active, setActive] = useState<TextFormatType[]>([]);
	const savedSelection = useRef<RangeSelection | null>(null);
	const toolbar = useRef<HTMLDivElement>(null);
	useEffect(() => {
		setDraft(null);
		savedSelection.current = null;
	}, [draftKey, disabled]);
	useEffect(
		() =>
			editor.registerUpdateListener(({ editorState }) => {
				editorState.read(() => {
					const selection = $getSelection();
					setInList($isComposerSelectionInList());
					if ($isRangeSelection(selection)) {
						savedSelection.current = selection.clone();
						const next = formats
							.filter(({ format }) => selection.hasFormat(format))
							.map(({ format }) => format);
						setActive((previous) =>
							previous.join() === next.join() ? previous : next,
						);
					}
				});
			}),
		[editor],
	);

	const $restoreSelection = useCallback(() => {
		const saved = savedSelection.current;
		if (
			saved &&
			$getNodeByKey(saved.anchor.key) &&
			$getNodeByKey(saved.focus.key)
		)
			$setSelection(saved.clone());
		else $getRoot().selectEnd();
	}, []);

	const beginLink = useCallback(() => {
		if (disabled) return;
		editor.update(() => {
			if (!$isRangeSelection($getSelection())) $restoreSelection();
			const selection = $getSelection();
			if (!$isRangeSelection(selection)) return;
			savedSelection.current = selection.clone();
			const link = $selectedLink();
			setError(false);
			setDraft({
				key: link?.getKey() ?? null,
				text: link?.getTextContent() ?? selection.getTextContent(),
				url: link?.getURL() ?? "",
				mode: "url",
				anchor: null,
			});
		});
	}, [$restoreSelection, disabled, editor]);

	useEffect(() => {
		// A link click opens its menu; prevent the browser from moving the caret
		// and stealing focus back from the popover during the same click.
		const mouseDown = (event: MouseEvent) => {
			if (
				!disabled &&
				event.target instanceof Element &&
				event.target.closest("a")
			)
				event.preventDefault();
		};
		const click = (event: MouseEvent) => {
			const anchor =
				event.target instanceof Element ? event.target.closest("a") : null;
			if (!anchor || !editor.getRootElement()?.contains(anchor)) return;
			event.preventDefault();
			if (disabled) return;
			editor.getEditorState().read(
				() => {
					let node = $getNearestNodeFromDOMNode(anchor);
					if (!$isLinkNode(node)) node = node?.getParent() ?? null;
					if (!$isLinkNode(node)) return;
					setError(false);
					setDraft({
						key: node.getKey(),
						text: node.getTextContent(),
						url: node.getURL(),
						mode: "menu",
						anchor: anchor.getBoundingClientRect(),
					});
				},
				{ editor },
			);
		};
		return editor.registerRootListener((root, previous) => {
			previous?.removeEventListener("mousedown", mouseDown);
			previous?.removeEventListener("click", click);
			root?.addEventListener("mousedown", mouseDown);
			root?.addEventListener("click", click);
		});
	}, [disabled, editor]);

	// Preserve source text when pasting code or web content. Images and large
	// snippets are handled first by PasteImagePlugin at critical priority.
	useEffect(
		() =>
			editor.registerCommand(
				PASTE_COMMAND,
				(event) => {
					if (
						disabled ||
						!(event instanceof ClipboardEvent) ||
						!event.clipboardData
					)
						return false;
					const text = event.clipboardData.getData("text/plain");
					const selection = $getSelection();
					if (!text || !$isRangeSelection(selection)) return false;
					event.preventDefault();
					const url = normalizeComposerUrl(text);
					if (url && !selection.isCollapsed()) $toggleLink(url);
					else selection.insertRawText(text);
					return true;
				},
				COMMAND_PRIORITY_HIGH,
			),
		[disabled, editor],
	);

	const save = () => {
		if (!draft || disabled) return;
		const url = normalizeComposerUrl(draft.url);
		if (!url) {
			setError(true);
			return;
		}
		editor.update(() => {
			if (draft.key) {
				const node = $getNodeByKey(draft.key);
				if (!$isLinkNode(node)) return;
				// Manual editing freezes an autolink's label and destination independently.
				const link = $createLinkNode(url);
				if (draft.text === node.getTextContent())
					link.append(...node.getChildren());
				else link.append($createTextNode(draft.text.trim() || url));
				node.replace(link);
				link.selectEnd();
			} else if (savedSelection.current) {
				$setSelection(savedSelection.current.clone());
				const selection = $getSelection();
				if (!$isRangeSelection(selection)) return;
				if (
					selection.isCollapsed() ||
					draft.text !== selection.getTextContent()
				) {
					selection.insertNodes([
						$createLinkNode(url).append(
							$createTextNode(draft.text.trim() || url),
						),
					]);
				} else $toggleLink(url);
			}
		});
		setDraft(null);
		editor.focus();
	};
	const remove = () => {
		if (!draft?.key || disabled) return;
		editor.update(() => {
			const node = $getNodeByKey(draft.key!);
			if (!$isLinkNode(node)) return;
			if ($isAutoLinkNode(node)) {
				node.setIsUnlinked(true);
				node.selectEnd();
			} else {
				const unlinked = $createAutoLinkNode(node.getURL(), {
					isUnlinked: true,
				});
				unlinked.append(...node.getChildren());
				node.replace(unlinked);
				unlinked.selectEnd();
			}
		});
		setDraft(null);
		editor.focus();
	};
	const open = async () => {
		const url = draft && normalizeComposerUrl(draft.url);
		if (!url) return;
		try {
			const result = await openExternal(url);
			if (!result.success) throw Error();
			setDraft(null);
		} catch {
			toast.error(label("openFailed"));
		}
	};

	return (
		<>
			<ComposerLinkIconsPlugin />
			<ListPlugin shouldPreserveNumbering />
			<MarkdownShortcutPlugin transformers={COMPOSER_LIST_TRANSFORMERS} />
			<LinkPlugin validateUrl={validateUrl} />
			<AutoLinkPlugin matchers={COMPOSER_LINK_MATCHERS} />
			<div
				className="composer-format-toolbar"
				ref={toolbar}
				role="group"
				aria-label={label("formatting")}
			>
				{formats.map(({ format, Icon, shortcut }) => (
					<Button
						key={format}
						type="button"
						variant="ghost"
						size="icon"
						className="size-7"
						disabled={disabled}
						aria-label={label(format)}
						aria-pressed={active.includes(format)}
						title={`${label(format)}${shortcut ? ` (⌘/Ctrl+${shortcut})` : ""}`}
						onMouseDown={(event) => event.preventDefault()}
						onClick={() => {
							editor.update(() => {
								$restoreSelection();
								editor.dispatchCommand(FORMAT_TEXT_COMMAND, format);
							});
							editor.focus();
						}}
					>
						<Icon className="size-3.5" />
					</Button>
				))}
				<span className="mx-1 h-3 border-l border-border" />
				<Button
					type="button"
					variant="ghost"
					size="icon"
					className="size-7"
					disabled={disabled}
					aria-label={label("insertLink")}
					title={label("insertLink")}
					onMouseDown={(event) => event.preventDefault()}
					onClick={beginLink}
				>
					<LinkIcon className="size-3.5" />
				</Button>
				{inList && (
					<span className="composer-list-hint">{label("listHint")}</span>
				)}
			</div>
			<Popover
				open={draft !== null}
				onOpenChange={(open) => {
					if (!open) setDraft(null);
				}}
			>
				<PopoverAnchor
					virtualRef={{
						current: {
							getBoundingClientRect: () =>
								draft?.anchor ??
								toolbar.current?.getBoundingClientRect() ??
								new DOMRect(),
						},
					}}
				/>
				<PopoverContent
					className="composer-link-popover"
					align="start"
					side="top"
					aria-label={label("link")}
					onCloseAutoFocus={(event) => {
						event.preventDefault();
						editor.focus();
					}}
				>
					{draft?.mode === "menu" ? (
						<>
							<div className="composer-link-heading">
								<SiteLinkIcon url={draft.url} className="composer-site-icon" />
								<span className="truncate font-medium">{draft.text}</span>
							</div>
							<p className="composer-link-destination">{draft.url}</p>
							<div className="composer-link-actions">
								<button type="button" onClick={() => void open()}>
									<ExternalLink />
									{label("openLink")}
								</button>
								<button
									type="button"
									onClick={() => setDraft({ ...draft, mode: "text" })}
								>
									<Pencil />
									{label("editText")}
								</button>
								<button
									type="button"
									onClick={() => setDraft({ ...draft, mode: "url" })}
								>
									<LinkIcon />
									{label("editLink")}
								</button>
								<button type="button" onClick={remove}>
									<Unlink />
									{label("removeLink")}
								</button>
							</div>
						</>
					) : (
						draft && (
							<form
								className="flex flex-col gap-3"
								onSubmit={(event) => {
									event.preventDefault();
									event.stopPropagation();
									save();
								}}
							>
								<label className="flex flex-col gap-1 text-xs">
									{label("text")}
									<input
										autoFocus={draft.mode === "text"}
										className="composer-link-input"
										value={draft.text}
										onChange={(event) =>
											setDraft({ ...draft, text: event.target.value })
										}
									/>
								</label>
								<label className="flex flex-col gap-1 text-xs">
									{label("url")}
									<input
										autoFocus={draft.mode === "url"}
										className="composer-link-input"
										placeholder="https://github.com/…"
										value={draft.url}
										aria-invalid={error}
										aria-describedby={error ? "composer-link-error" : undefined}
										onChange={(event) => {
											setError(false);
											setDraft({ ...draft, url: event.target.value });
										}}
									/>
								</label>
								{error && (
									<p
										id="composer-link-error"
										role="alert"
										className="text-xs text-destructive"
									>
										{label("invalidUrl")}
									</p>
								)}
								<div className="flex justify-end gap-2">
									<Button
										type="button"
										variant="ghost"
										size="sm"
										onClick={() => setDraft(null)}
									>
										{label("cancel")}
									</Button>
									<Button type="submit" size="sm">
										{label("save")}
									</Button>
								</div>
							</form>
						)
					)}
				</PopoverContent>
			</Popover>
		</>
	);
}
