import { PanelLeft, PanelRight } from "lucide-react";
import { Badge } from "../../components/ui/badge";
import { Button } from "../../components/ui/button";
import { Input } from "../../components/ui/input";
import { Label } from "../../components/ui/label";
import { ScrollArea } from "../../components/ui/scroll-area";
import { Switch } from "../../components/ui/switch";
import { Tooltip, TooltipContent, TooltipTrigger } from "../../components/ui/tooltip";
import { getWorkspaceTone } from "./data";
import type { WorkspaceSummary } from "./types";

type WorkspacesSidebarProps = {
	collapsed: boolean;
	filter: string;
	onFilterChange: (value: string) => void;
	onSelectWorkspace: (workspaceId: string) => void;
	onShowArchivedChange: (value: boolean) => void;
	onCreateWorkspace: () => void;
	onToggleCollapsed: () => void;
	selectedWorkspaceId: string;
	sidebarWidth: number;
	showArchived: boolean;
	workspaces: WorkspaceSummary[];
};

export function WorkspacesSidebar({
	collapsed,
	filter,
	onFilterChange,
	onSelectWorkspace,
	onShowArchivedChange,
	onCreateWorkspace,
	onToggleCollapsed,
	selectedWorkspaceId,
	sidebarWidth,
	showArchived,
	workspaces,
}: WorkspacesSidebarProps) {
	const groupedWorkspaces = [
		{
			label: "Done",
			statuses: ["ready"] as const,
			items: workspaces.filter((workspace) => workspace.status === "ready"),
		},
		{
			label: "In review",
			statuses: ["setup_pending"] as const,
			items: workspaces.filter((workspace) => workspace.status === "setup_pending"),
		},
		{
			label: "In progress",
			statuses: ["initializing"] as const,
			items: workspaces.filter((workspace) => workspace.status === "initializing"),
		},
		{
			label: "Archived",
			statuses: ["archived"] as const,
			items: workspaces.filter((workspace) => workspace.status === "archived"),
		},
	].filter((group) => group.items.length > 0);

	return (
		<>
			<div className="dcc-sidebar__header">
				<div className="dcc-brand">
					<div className="dcc-brand__mark" aria-hidden="true" />
					<div>
						<p className="dcc-eyebrow">Dev Command Center</p>
						<h1>Workspace shell</h1>
					</div>
				</div>
				<Tooltip>
					<TooltipTrigger asChild>
						<Button
							type="button"
							variant="ghost"
							size="icon"
							onClick={onToggleCollapsed}
							aria-label="Toggle sidebar"
						>
							{collapsed ? <PanelRight /> : <PanelLeft />}
						</Button>
					</TooltipTrigger>
					<TooltipContent>Toggle sidebar</TooltipContent>
				</Tooltip>
			</div>

			<div className="dcc-sidebar__section">
				<Label>Workspaces</Label>
				<Input
					placeholder="Filter workspaces"
					value={filter}
					onChange={(event) => onFilterChange(event.target.value)}
				/>
				<ScrollArea className="dcc-workspace-list">
					<div className="dcc-workspace-list__groups">
						{groupedWorkspaces.map((group) => (
							<section key={group.label} className="dcc-workspace-group">
								<div className="dcc-workspace-group__header">
									<span>{group.label}</span>
									<Badge variant="outline">{group.items.length}</Badge>
								</div>
								{group.items.map((workspace) => (
									<button
										key={workspace.id}
										className="dcc-workspace-card"
										type="button"
										data-active={workspace.id === selectedWorkspaceId}
										onClick={() => onSelectWorkspace(workspace.id)}
									>
										<div className="dcc-card__header">
											<strong>{workspace.name}</strong>
											<Badge variant={getWorkspaceTone(workspace.status)}>
												{workspace.status}
											</Badge>
										</div>
										<span>{workspace.branch}</span>
										<small>{workspace.id}</small>
									</button>
								))}
							</section>
						))}
					</div>
				</ScrollArea>
			</div>

			<div className="dcc-sidebar__section">
				<Label>Layout</Label>
				<div className="dcc-slider-row">
					<span>Sidebar width</span>
					<input type="range" min="240" max="520" value={sidebarWidth} readOnly />
				</div>
				<div className="dcc-slider-row dcc-switch-row">
					<div className="dcc-switch-row__label">
						<span>Show archived</span>
						<small>Keep old workspaces visible in the list.</small>
					</div>
					<Switch checked={showArchived} onCheckedChange={onShowArchivedChange} />
				</div>
				<p className="dcc-sidebar__hint">
					Drag the divider or use arrow keys while it is focused.
				</p>
			</div>

			<div className="dcc-sidebar__footer">
				<Button type="button" className="dcc-sidebar__cta" onClick={onCreateWorkspace}>
					New workspace
				</Button>
				<Badge variant="outline">Helmor layout</Badge>
			</div>
		</>
	);
}
