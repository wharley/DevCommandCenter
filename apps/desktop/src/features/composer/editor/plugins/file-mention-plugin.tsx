import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import {
	LexicalTypeaheadMenuPlugin,
	MenuOption,
} from "@lexical/react/LexicalTypeaheadMenuPlugin";
import { useQuery } from "@tanstack/react-query";
import { $createTextNode, type TextNode } from "lexical";
import { useTranslation } from "react-i18next";
import { FileText } from "lucide-react";
import {
	type RefObject,
	type ReactNode,
	type CSSProperties,
	useLayoutEffect,
	useCallback,
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
import type { TrackedComposerFile } from "../../workspace-tracked-files-query";
import { workspaceTrackedFilesQueryOptions } from "../../workspace-tracked-files-query";
import { $createFileBadgeNode } from "../file-badge-node";

export const MAX_VISIBLE_OPTIONS = 50;
const MAX_FILE_MENTION_LENGTH = 200;

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
	if (name === q || path === q) {
		return 5;
	}
	if (path.endsWith(`/${q}`)) {
		return 4;
	}
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
	const q = query.toLowerCase();
	// Once a complete filename (or path) is typed, do not dilute the result
	// list with similarly named files such as paymentPix.ts or redis.tsx.
	const exactMatches = files.filter((file) => {
		const name = file.name.toLowerCase();
		const path = file.path.toLowerCase();
		return name === q || path === q;
	});
	if (exactMatches.length > 0) {
		return exactMatches.slice(0, MAX_VISIBLE_OPTIONS);
	}
	const ranked = files
		.map((file) => ({ file, score: rankFile(file, query) }))
		.filter((entry) => entry.score > 0);
	ranked.sort((a, b) => b.score - a.score);
	return ranked.slice(0, MAX_VISIBLE_OPTIONS).map((entry) => entry.file);
}

/**
 * File paths commonly contain punctuation (for example payment.ts and
 * app/[token]/route.ts). Lexical's generic trigger matcher treats that
 * punctuation as a mention terminator, so use a path-aware matcher instead.
 */
export function matchFileMentionTrigger(text: string) {
	const match = new RegExp(
		`(^|\\s|\\()(@([^\\s@]{0,${MAX_FILE_MENTION_LENGTH}}))$`,
	).exec(text);
	if (!match) {
		return null;
	}
	const leadingCharacter = match[1];
	return {
		leadOffset: match.index + leadingCharacter.length,
		matchingString: match[3],
		replaceableString: match[2],
	};
}

export function FileMentionPlugin({
	workspaceRootPath,
	popupAnchorRef,
	floating = false,
}: {
	workspaceRootPath: string | null;
	floating?: boolean;
	popupAnchorRef?: RefObject<HTMLElement | null>;
}) {
	const [editor] = useLexicalComposerContext();
	const { t } = useTranslation("common");
	const [query, setQuery] = useState<string | null>(null);

	const filesQuery = useQuery(
		workspaceTrackedFilesQueryOptions(workspaceRootPath),
	);
	const files = filesQuery.data ?? [];

	const options = useMemo(() => {
		const filtered = filterFiles(files, query ?? "");
		return filtered.map((file) => new FileMentionOption(file));
	}, [files, query]);

	const triggerFn = useCallback(matchFileMentionTrigger, []);

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
				const { selectedIndex, selectOptionAndCleanUp, setHighlightedIndex } =
					menuProps;
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
					<MentionPopup anchor={portalTarget} floating={floating}>
						<Command
							value={highlightValue}
							shouldFilter={false}
							className="rounded-xl border border-border/60 bg-background text-foreground shadow-2xl ring-1 ring-black/5"
						>
							<CommandList className="max-h-[var(--mention-list-height,18rem)]">
								<CommandGroup heading={t("composer.fileMentions")}>
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
					</MentionPopup>,
					floating ? document.body : portalTarget,
				);
			}}
		/>
	);
}

/** Escape scroll-container clipping while fitting the small quick-entry window. */
function MentionPopup({
	anchor,
	floating,
	children,
}: {
	anchor: HTMLElement;
	floating: boolean;
	children: ReactNode;
}) {
	const [style, setStyle] = useState<
		CSSProperties & { "--mention-list-height": string }
	>();
	useLayoutEffect(() => {
		if (!floating) return;
		const position = () => {
			const rect = anchor.getBoundingClientRect();
			const above = Math.max(0, rect.top - 16);
			const below = Math.max(0, window.innerHeight - rect.bottom - 16);
			const upwards = above >= below;
			const width = Math.min(640, window.innerWidth - 32);
			setStyle({
				position: "fixed",
				left: Math.max(16, Math.min(rect.left, window.innerWidth - width - 16)),
				width,
				...(upwards
					? { bottom: window.innerHeight - rect.top + 8 }
					: { top: rect.bottom + 8 }),
				"--mention-list-height": `${Math.max(0, Math.min(288, upwards ? above : below) - 10)}px`,
			});
		};
		position();
		const observer = new ResizeObserver(position);
		observer.observe(anchor);
		window.addEventListener("resize", position);
		window.addEventListener("scroll", position, true);
		return () => {
			observer.disconnect();
			window.removeEventListener("resize", position);
			window.removeEventListener("scroll", position, true);
		};
	}, [anchor, floating]);
	return (
		<div
			data-typeahead-popup="mention"
			data-dcc-browser-occluder="true"
			style={floating ? style : undefined}
			className={cn(
				"pointer-events-auto isolate z-[9999]",
				!floating &&
					"absolute bottom-full left-0 mb-2 w-[min(640px,calc(100vw-2rem))]",
			)}
		>
			{children}
		</div>
	);
}
