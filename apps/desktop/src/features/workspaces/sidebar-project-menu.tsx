import { GitBranch, Plus, Settings2, Trash2 } from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { Repository } from "@dcc/contracts";
import {
	ContextMenu,
	ContextMenuContent,
	ContextMenuItem,
	ContextMenuSeparator,
	ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { repositoryDisplayName } from "./repository-display-name";

/**
 * The rail has no project headers: a task row carries its project, and a
 * right click on it opens what the header's menu used to offer.
 */
export function SidebarProjectMenu({
	repository,
	children,
	onNewTask,
	onOpenBranch,
	onEdit,
	onRemove,
}: {
	repository: Repository | null;
	children: ReactNode;
	onNewTask?: (repository: Repository) => void;
	onOpenBranch?: (repository: Repository) => void;
	onEdit?: (repository: Repository) => void;
	onRemove?: (repository: Repository) => void;
}) {
	const { t } = useTranslation("common");
	if (!repository || !(onNewTask || onOpenBranch || onEdit || onRemove)) {
		return children;
	}
	const label = repositoryDisplayName(repository);
	return (
		<ContextMenu>
			<ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
			<ContextMenuContent className="min-w-48">
				<div className="truncate px-1.5 pb-1 pt-0.5 text-[11px] font-medium text-muted-foreground">
					{label}
				</div>
				{onNewTask ? (
					<ContextMenuItem className="gap-2 text-[13px]" onSelect={() => onNewTask(repository)}>
						<Plus className="size-3.5" strokeWidth={1.9} aria-hidden />
						{t("sidebar.projectMenu.newTask")}
					</ContextMenuItem>
				) : null}
				{onOpenBranch ? (
					<ContextMenuItem className="gap-2 text-[13px]" onSelect={() => onOpenBranch(repository)}>
						<GitBranch className="size-3.5" strokeWidth={1.9} aria-hidden />
						{t("sidebar.openBranch")}
					</ContextMenuItem>
				) : null}
				{onEdit || onRemove ? <ContextMenuSeparator /> : null}
				{onEdit ? (
					<ContextMenuItem className="gap-2 text-[13px]" onSelect={() => onEdit(repository)}>
						<Settings2 className="size-3.5" strokeWidth={1.9} aria-hidden />
						{t("sidebar.editProject")}
					</ContextMenuItem>
				) : null}
				{onRemove ? (
					<ContextMenuItem
						className="gap-2 text-[13px] text-destructive focus:text-destructive"
						onSelect={() => onRemove(repository)}
					>
						<Trash2 className="size-3.5" strokeWidth={2} aria-hidden />
						{t("sidebar.removeProject")}
					</ContextMenuItem>
				) : null}
			</ContextMenuContent>
		</ContextMenu>
	);
}
