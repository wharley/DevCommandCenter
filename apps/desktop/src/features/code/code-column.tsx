import { PanelLeftClose, Search } from "lucide-react";
import { useMemo, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { InlineShortcutDisplay } from "@/features/shortcuts/InlineShortcutDisplay";
import {
	getCodeColumnShortcutKeys,
	getQuickOpenShortcutKeys,
} from "@/features/shortcuts/shortcut-utils";
import { useWorkspaceGitStatus } from "@/features/inspector/use-workspace-git-status";
import { cn } from "@/lib/utils";
import { CodeFilesTree } from "./code-files-tree";
import { setCodeColumnTab, useCodeColumnTab, type CodeColumnTab } from "./code-column-store";

export const CODE_COLUMN_WIDTH = 260;

/**
 * The project's files and changes next to the tasks sidebar. Lists only:
 * picking a file or a change opens it as a tab in the center of the workbench.
 */
export function CodeColumn({
	workspaceRoot,
	changesRoot,
	selectedPath,
	changes,
	onOpenFile,
	onOpenQuickOpen,
	onClose,
}: {
	workspaceRoot: string | null;
	/** Where the task's changes live (its session worktree when it has one). */
	changesRoot: string | null;
	selectedPath: string | null;
	/** The changes list, built by the app with its review handlers. */
	changes: ReactNode;
	onOpenFile: (input: { path: string; name: string }) => void;
	onOpenQuickOpen: () => void;
	onClose: () => void;
}) {
	const { t } = useTranslation("common");
	const activeTab = useCodeColumnTab();
	const gitStatus = useWorkspaceGitStatus(changesRoot);
	const changeCount = useMemo(() => {
		const data = gitStatus.data;
		if (!data) return 0;
		return new Set([...data.staged, ...data.unstaged].map((entry) => entry.path)).size;
	}, [gitStatus.data]);
	const tabs: { id: CodeColumnTab; label: string; count?: number }[] = [
		{ id: "files", label: t("codeColumn.files") },
		{ id: "changes", label: t("codeColumn.changes"), count: changeCount },
	];
	return (
		<aside
			aria-label={t("codeColumn.ariaLabel")}
			className="flex h-full shrink-0 flex-col overflow-hidden border-r border-border/60 bg-sidebar"
			style={{ width: CODE_COLUMN_WIDTH }}
		>
			<div className="flex h-10 shrink-0 items-center gap-1 border-b border-border/60 pl-2 pr-1.5">
				<div role="tablist" className="flex min-w-0 flex-1 items-center gap-0.5">
					{tabs.map((tab) => (
						<button
							key={tab.id}
							type="button"
							role="tab"
							aria-selected={tab.id === activeTab}
							onClick={() => setCodeColumnTab(tab.id)}
							className={cn(
								"flex min-w-0 cursor-pointer items-center gap-1 rounded-md px-2 py-1 transition-colors",
								tab.id === activeTab
									? "bg-muted/55 text-foreground"
									: "text-muted-foreground hover:bg-muted/30 hover:text-foreground",
							)}
						>
							<span className="truncate text-[12px] font-medium">{tab.label}</span>
							{tab.count ? (
								<span className="text-[10px] tabular-nums text-amber-500">{tab.count}</span>
							) : null}
						</button>
					))}
				</div>
				<Tooltip>
					<TooltipTrigger asChild>
						<Button
							type="button"
							variant="ghost"
							size="icon-xs"
							className="size-7 shrink-0 cursor-pointer text-muted-foreground hover:text-foreground"
							onClick={onOpenQuickOpen}
							aria-label={t("inspector.codeSection.quickOpen")}
						>
							<Search className="size-3.5" strokeWidth={1.8} />
						</Button>
					</TooltipTrigger>
					<TooltipContent side="bottom" className="gap-2">
						<span>{t("inspector.codeSection.quickOpen")}</span>
						<InlineShortcutDisplay keys={getQuickOpenShortcutKeys()} />
					</TooltipContent>
				</Tooltip>
				<Tooltip>
					<TooltipTrigger asChild>
						<Button
							type="button"
							variant="ghost"
							size="icon-xs"
							className="size-7 shrink-0 cursor-pointer text-muted-foreground hover:text-foreground"
							onClick={onClose}
							aria-label={t("codeColumn.close")}
						>
							<PanelLeftClose className="size-3.5" strokeWidth={1.8} />
						</Button>
					</TooltipTrigger>
					<TooltipContent side="bottom" className="gap-2">
						<span>{t("codeColumn.close")}</span>
						<InlineShortcutDisplay keys={getCodeColumnShortcutKeys()} />
					</TooltipContent>
				</Tooltip>
			</div>
			{activeTab === "files" ? (
				<CodeFilesTree
					workspaceRoot={workspaceRoot}
					selectedPath={selectedPath}
					showHeader={false}
					onOpenFile={onOpenFile}
					onOpenQuickOpen={onOpenQuickOpen}
				/>
			) : (
				<div className="flex min-h-0 flex-1 flex-col overflow-hidden">{changes}</div>
			)}
		</aside>
	);
}
