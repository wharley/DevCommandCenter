import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import {
	LexicalTypeaheadMenuPlugin,
	MenuOption,
	useBasicTypeaheadTriggerMatch,
} from "@lexical/react/LexicalTypeaheadMenuPlugin";
import { useQuery } from "@tanstack/react-query";
import { $createTextNode, type TextNode } from "lexical";
import { FileText } from "lucide-react";
import { type RefObject, useCallback, useMemo, useState } from "react";
import { createPortal } from "react-dom";
import {
	Command,
	CommandEmpty,
	CommandGroup,
	CommandItem,
	CommandList,
} from "@/components/ui/command";
import { cn } from "@/lib/utils";
import type { TrackedComposerFile } from "../../workspace-tracked-files-query";
import { workspaceTrackedFilesQueryOptions } from "../../workspace-tracked-files-query";
import { $createFileBadgeNode } from "../file-badge-node";

export const MAX_VISIBLE_OPTIONS = 50;

class FileMentionOption extends MenuOption {
	readonly file: TrackedComposerFile;
	constructor(file: TrackedComposerFile) {
		super(file.path);
		this.file = file;
	}
}

export function rankFile(file: TrackedComposerFile, query: string): number {
	if (!query) {
		return 1;
	}
	const q = query.toLowerCase();
	const name = file.name.toLowerCase();
	const path = file.path.toLowerCase();
	if (name.startsWith(q)) {
		return 3;
	}
	if (name.includes(q)) {
		return 2;
	}
	if (path.includes(q)) {
		return 1;
	}
	return 0;
}

export function filterFiles(
	files: readonly TrackedComposerFile[],
	query: string,
): TrackedComposerFile[] {
	if (!query) {
		return files.slice(0, MAX_VISIBLE_OPTIONS);
	}
	const ranked = files
		.map((file) => ({ file, score: rankFile(file, query) }))
		.filter((entry) => entry.score > 0);
	ranked.sort((a, b) => b.score - a.score);
	return ranked.slice(0, MAX_VISIBLE_OPTIONS).map((entry) => entry.file);
}

export function FileMentionPlugin({
	workspaceRootPath,
	popupAnchorRef,
}: {
	workspaceRootPath: string | null;
	popupAnchorRef?: RefObject<HTMLElement | null>;
}) {
	const [editor] = useLexicalComposerContext();
	const [query, setQuery] = useState<string | null>(null);

	const filesQuery = useQuery(workspaceTrackedFilesQueryOptions(workspaceRootPath));
	const files = filesQuery.data ?? [];

	const options = useMemo(() => {
		const filtered = filterFiles(files, query ?? "");
		return filtered.map((file) => new FileMentionOption(file));
	}, [files, query]);

	const triggerFn = useBasicTypeaheadTriggerMatch("@", {
		minLength: 0,
	});

	const onSelectOption = useCallback(
		(
			selected: FileMentionOption,
			nodeToReplace: TextNode | null,
			closeMenu: () => void,
		) => {
			editor.update(() => {
				if (nodeToReplace) {
					const badge = $createFileBadgeNode(selected.file.path);
					const trailing = $createTextNode(" ");
					nodeToReplace.replace(badge);
					badge.insertAfter(trailing);
					trailing.select(1, 1);
				}
				closeMenu();
			});
		},
		[editor],
	);

	return (
		<LexicalTypeaheadMenuPlugin<FileMentionOption>
			triggerFn={triggerFn}
			onQueryChange={setQuery}
			onSelectOption={onSelectOption}
			options={options}
			anchorClassName="file-mention-anchor"
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

				const highlightValue = options[selectedIndex ?? 0]?.file.path ?? "";

				return createPortal(
					<div
						data-typeahead-popup="mention"
						className="pointer-events-auto absolute bottom-full left-0 isolate z-[9999] mb-2 w-[min(640px,calc(100vw-2rem))]"
					>
						<Command
							value={highlightValue}
							shouldFilter={false}
							className="rounded-xl border border-border/60 bg-background text-foreground shadow-2xl ring-1 ring-black/5"
						>
							<CommandList className="max-h-72">
								<CommandEmpty>No files</CommandEmpty>
								<CommandGroup heading="Files">
									{options.map((opt, index) => {
										const file = opt.file;
										const isSelected = index === selectedIndex;
										const lastSlash = file.path.lastIndexOf("/");
										const directory =
											lastSlash >= 0 ? file.path.slice(0, lastSlash + 1) : "";
										return (
											<CommandItem
												key={opt.key}
												value={file.path}
												ref={(el) => opt.setRefElement(el)}
												onSelect={() => selectOptionAndCleanUp(opt)}
												onMouseEnter={() => setHighlightedIndex(index)}
												onPointerDown={(event) => event.preventDefault()}
												className={cn(
													"min-w-0 rounded-lg px-2.5 py-2 text-[13px]",
													isSelected && "bg-muted text-foreground",
												)}
											>
												<FileText
													className="size-3.5 shrink-0 text-muted-foreground"
													strokeWidth={1.8}
												/>
												<span className="min-w-0 shrink-0 truncate font-medium">
													{file.name}
												</span>
												<span
													className="min-w-0 flex-1 truncate whitespace-nowrap text-xs text-muted-foreground"
													title={file.path}
												>
													{directory}
												</span>
											</CommandItem>
										);
									})}
								</CommandGroup>
							</CommandList>
						</Command>
					</div>,
					portalTarget,
				);
			}}
		/>
	);
}
