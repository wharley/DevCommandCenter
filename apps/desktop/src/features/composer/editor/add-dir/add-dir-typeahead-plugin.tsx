import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import {
	LexicalTypeaheadMenuPlugin,
	MenuOption,
} from "@lexical/react/LexicalTypeaheadMenuPlugin";
import {
	$getSelection,
	$isRangeSelection,
	$isTextNode,
	COMMAND_PRIORITY_LOW,
	KEY_BACKSPACE_COMMAND,
	KEY_ESCAPE_COMMAND,
	type LexicalEditor,
	type TextNode,
} from "lexical";
import { FolderOpen } from "lucide-react";
import {
	type ReactNode,
	type RefObject,
	useCallback,
	useEffect,
	useMemo,
	useState,
} from "react";
import { createPortal } from "react-dom";
import {
	Command,
	CommandGroup,
	CommandItem,
	CommandList,
} from "@/components/ui/command";
import { cn } from "@/lib/utils";
import type { WorkspaceChildDirCandidate } from "../../workspace-child-dirs-query";
import { $isAddDirTriggerNode } from "./trigger-node";

const QUERY_LEAD_PATTERN = /^\s*/;

export type AddDirPickEntry =
	| { kind: "browse" }
	| {
			kind: "candidate";
			candidate: WorkspaceChildDirCandidate;
			alreadyLinked: boolean;
	  };

class AddDirOption extends MenuOption {
	readonly entry: AddDirPickEntry;
	constructor(entry: AddDirPickEntry) {
		super(
			entry.kind === "browse" ? "__browse__" : entry.candidate.absolutePath,
		);
		this.entry = entry;
	}
}

function rankCandidate(c: WorkspaceChildDirCandidate, q: string): number {
	if (!q) {
		return 1;
	}
	const lower = q.toLowerCase();
	const title = c.title.toLowerCase();
	const path = c.absolutePath.toLowerCase();
	if (title.startsWith(lower)) {
		return 4;
	}
	if (path.includes(lower) || title.includes(lower)) {
		return 2;
	}
	return 0;
}

export function filterAddDirCandidates(
	candidates: readonly WorkspaceChildDirCandidate[],
	query: string,
): WorkspaceChildDirCandidate[] {
	if (!query) {
		return [...candidates];
	}
	const ranked = candidates
		.map((c) => ({ c, score: rankCandidate(c, query) }))
		.filter((r) => r.score > 0)
		.sort((a, b) => b.score - a.score);
	return ranked.map((r) => r.c);
}

function $findActiveQueryNode(): {
	textNode: TextNode;
	leadingWhitespaceLen: number;
} | null {
	const selection = $getSelection();
	if (!$isRangeSelection(selection) || !selection.isCollapsed()) {
		return null;
	}
	const node = selection.anchor.getNode();
	if (!$isTextNode(node)) {
		return null;
	}
	const prev = node.getPreviousSibling();
	if (!$isAddDirTriggerNode(prev)) {
		return null;
	}
	const text = node.getTextContent();
	const match = QUERY_LEAD_PATTERN.exec(text);
	const leadingWhitespaceLen = match ? match[0].length : 0;
	return { textNode: node, leadingWhitespaceLen };
}

function $exitAddDirMode(editor: LexicalEditor) {
	editor.update(() => {
		const selection = $getSelection();
		if (!$isRangeSelection(selection)) {
			return;
		}
		const node = selection.anchor.getNode();
		if (!$isTextNode(node)) {
			return;
		}
		const prev = node.getPreviousSibling();
		if (!$isAddDirTriggerNode(prev)) {
			return;
		}
		prev.remove();
		node.remove();
	});
}

export function AddDirTypeaheadPlugin({
	candidates,
	linkedDirectoryPaths,
	onPick,
	popupAnchorRef,
}: {
	candidates: readonly WorkspaceChildDirCandidate[];
	linkedDirectoryPaths: readonly string[];
	onPick: (entry: AddDirPickEntry) => void;
	popupAnchorRef?: RefObject<HTMLElement | null>;
}) {
	const [editor] = useLexicalComposerContext();
	const [query, setQuery] = useState<string | null>(null);

	useEffect(() => {
		return editor.registerCommand<KeyboardEvent>(
			KEY_BACKSPACE_COMMAND,
			(event) => {
				const found = editor
					.getEditorState()
					.read(() => $findActiveQueryNode());
				if (!found) {
					return false;
				}
				const selection = editor.getEditorState().read(() => $getSelection());
				if (!selection || !$isRangeSelection(selection)) {
					return false;
				}
				const offset = selection.anchor.offset;
				if (offset > found.leadingWhitespaceLen) {
					return false;
				}
				event?.preventDefault();
				$exitAddDirMode(editor);
				return true;
			},
			COMMAND_PRIORITY_LOW,
		);
	}, [editor]);

	useEffect(() => {
		return editor.registerCommand<KeyboardEvent>(
			KEY_ESCAPE_COMMAND,
			(event) => {
				const found = editor
					.getEditorState()
					.read(() => $findActiveQueryNode());
				if (!found) {
					return false;
				}
				event?.preventDefault();
				$exitAddDirMode(editor);
				return true;
			},
			COMMAND_PRIORITY_LOW,
		);
	}, [editor]);

	const triggerFn = useCallback(
		(text: string) => {
			const found = editor.getEditorState().read(() => $findActiveQueryNode());
			if (!found) {
				return null;
			}
			const leadingWsLen = found.leadingWhitespaceLen;
			return {
				leadOffset: 0,
				matchingString: text.slice(leadingWsLen),
				replaceableString: text,
			};
		},
		[editor],
	);

	const options = useMemo(() => {
		const linkedSet = new Set(linkedDirectoryPaths);
		const filtered = filterAddDirCandidates(candidates, query ?? "");
		const rows: AddDirPickEntry[] = [{ kind: "browse" }];
		for (const c of filtered) {
			rows.push({
				kind: "candidate",
				candidate: c,
				alreadyLinked: linkedSet.has(c.absolutePath),
			});
		}
		return rows.map((entry) => new AddDirOption(entry));
	}, [candidates, linkedDirectoryPaths, query]);

	const onSelectOption = useCallback(
		(selected: AddDirOption, _node: TextNode | null, closeMenu: () => void) => {
			$exitAddDirMode(editor);
			closeMenu();
			onPick(selected.entry);
		},
		[editor, onPick],
	);

	return (
		<LexicalTypeaheadMenuPlugin<AddDirOption>
			triggerFn={triggerFn}
			onQueryChange={setQuery}
			onSelectOption={onSelectOption}
			options={options}
			anchorClassName="add-dir-anchor"
			menuRenderFn={(anchorElementRef, menuProps) => {
				const {
					selectedIndex,
					selectOptionAndCleanUp,
					setHighlightedIndex,
				} = menuProps;
				const portalTarget =
					popupAnchorRef?.current ?? anchorElementRef.current;
				if (!portalTarget) {
					return null;
				}
				if (options.length === 0) {
					return null;
				}
				const highlightValue = options[selectedIndex ?? 0]?.key ?? "";
				return createPortal(
					<div
						data-typeahead-popup="add-dir"
						className="pointer-events-auto absolute bottom-full left-0 isolate z-[9999] mb-2 w-[min(640px,calc(100vw-2rem))]"
					>
						<Command
							value={highlightValue}
							shouldFilter={false}
							className="rounded-xl border border-border/60 bg-background text-foreground shadow-2xl ring-1 ring-black/5"
						>
							<CommandList className="max-h-72">
								<CommandGroup heading="Add working directory">
									{options.map((opt, index) => (
										<AddDirPickerRow
											key={opt.key}
											option={opt}
											isSelected={index === selectedIndex}
											setRef={opt.setRefElement.bind(opt)}
											onSelect={() => selectOptionAndCleanUp(opt)}
											onMouseEnter={() => setHighlightedIndex(index)}
										/>
									))}
								</CommandGroup>
							</CommandList>
							<div className="border-t border-border/40 px-3 py-1.5 font-mono text-[11px] text-muted-foreground">
								<span>↑↓ navigate · ↵ select · esc cancel</span>
							</div>
						</Command>
					</div>,
					portalTarget,
				);
			}}
		/>
	);
}

function AddDirPickerRow({
	option,
	isSelected,
	setRef,
	onSelect,
	onMouseEnter,
}: {
	option: AddDirOption;
	isSelected: boolean;
	setRef: (el: HTMLElement | null) => void;
	onSelect: () => void;
	onMouseEnter: () => void;
}): ReactNode {
	const entry = option.entry;
	const commonCn = cn(
		"min-w-0 gap-2.5 rounded-lg px-2.5 py-2 text-[13px]",
		isSelected && "bg-muted text-foreground",
	);
	if (entry.kind === "browse") {
		return (
			<CommandItem
				value={option.key}
				ref={setRef}
				onSelect={onSelect}
				onMouseEnter={onMouseEnter}
				onPointerDown={(event) => event.preventDefault()}
				className={commonCn}
			>
				<FolderOpen
					className="size-4 shrink-0 text-muted-foreground"
					strokeWidth={1.8}
				/>
				<span className="min-w-0 flex-1 truncate font-medium text-muted-foreground">
					Browse folder…
				</span>
				<span className="shrink-0 whitespace-nowrap text-xs text-muted-foreground">
					pick any directory
				</span>
			</CommandItem>
		);
	}
	const c = entry.candidate;
	return (
		<CommandItem
			value={option.key}
			ref={setRef}
			onSelect={onSelect}
			onMouseEnter={onMouseEnter}
			onPointerDown={(event) => event.preventDefault()}
			className={commonCn}
		>
			<FolderOpen
				className="size-4 shrink-0 text-muted-foreground"
				strokeWidth={1.8}
			/>
			<span className="min-w-0 flex-1 truncate font-medium" title={c.absolutePath}>
				{c.title}
			</span>
			{entry.alreadyLinked ? (
				<span className="ml-1 shrink-0 font-mono text-[10px] text-muted-foreground">
					linked
				</span>
			) : null}
		</CommandItem>
	);
}
