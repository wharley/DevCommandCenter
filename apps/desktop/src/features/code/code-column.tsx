import { PanelLeftClose, Search } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { InlineShortcutDisplay } from "@/features/shortcuts/InlineShortcutDisplay";
import {
	getCodeColumnShortcutKeys,
	getQuickOpenShortcutKeys,
} from "@/features/shortcuts/shortcut-utils";
import { CodeFilesTree } from "./code-files-tree";

export const CODE_COLUMN_WIDTH = 260;

/**
 * The project's files next to the tasks sidebar. Lists only: picking a file
 * opens it as a tab in the center of the workbench.
 */
export function CodeColumn({
	workspaceRoot,
	selectedPath,
	onOpenFile,
	onOpenQuickOpen,
	onClose,
}: {
	workspaceRoot: string | null;
	selectedPath: string | null;
	onOpenFile: (input: { path: string; name: string }) => void;
	onOpenQuickOpen: () => void;
	onClose: () => void;
}) {
	const { t } = useTranslation("common");
	return (
		<aside
			aria-label={t("codeColumn.ariaLabel")}
			className="flex h-full shrink-0 flex-col overflow-hidden border-r border-border/60 bg-sidebar"
			style={{ width: CODE_COLUMN_WIDTH }}
		>
			<div className="flex h-10 shrink-0 items-center gap-1 border-b border-border/60 pl-2 pr-1.5">
				<div role="tablist" className="flex min-w-0 flex-1 items-center gap-0.5">
					<span
						role="tab"
						aria-selected
						className="truncate rounded-md bg-muted/55 px-2 py-1 text-[12px] font-medium text-foreground"
					>
						{t("codeColumn.files")}
					</span>
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
			<CodeFilesTree
				workspaceRoot={workspaceRoot}
				selectedPath={selectedPath}
				showHeader={false}
				onOpenFile={onOpenFile}
				onOpenQuickOpen={onOpenQuickOpen}
			/>
		</aside>
	);
}
