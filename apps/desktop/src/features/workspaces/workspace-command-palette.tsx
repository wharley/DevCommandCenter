import { useMemo } from "react";
import {
	CommandDialog,
	CommandEmpty,
	CommandGroup,
	CommandInput,
	CommandItem,
	CommandList,
	CommandSeparator,
	CommandShortcut,
} from "../../components/ui/command";
import type { WorkspaceSummary } from "./types";

type WorkspaceCommandPaletteProps = {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	workspaces: WorkspaceSummary[];
	selectedWorkspaceId: string;
	onSelectWorkspace: (workspaceId: string) => void;
};

export function WorkspaceCommandPalette({
	open,
	onOpenChange,
	workspaces,
	selectedWorkspaceId,
	onSelectWorkspace,
}: WorkspaceCommandPaletteProps) {
	const groupedWorkspaces = useMemo(
		() => [
			{
				label: "Switch workspace",
				items: workspaces,
			},
		],
		[workspaces],
	);

	return (
		<CommandDialog
			open={open}
			onOpenChange={onOpenChange}
			title="Workspace palette"
			description="Search and switch workspaces."
		>
			<CommandInput placeholder="Search workspaces..." />
			<CommandList>
				<CommandEmpty>No workspace found.</CommandEmpty>
				{groupedWorkspaces.map((group) => (
					<CommandGroup key={group.label} heading={group.label}>
						{group.items.map((workspace) => (
							<CommandItem
								key={workspace.id}
								value={`${workspace.name} ${workspace.branch} ${workspace.id}`}
								onSelect={() => {
									onSelectWorkspace(workspace.id);
									onOpenChange(false);
								}}
							>
								<strong className="truncate">{workspace.name}</strong>
								<span className="truncate text-[var(--dcc-text-muted)]">
									{workspace.branch}
								</span>
								{workspace.id === selectedWorkspaceId ? (
									<CommandShortcut>Selected</CommandShortcut>
								) : null}
							</CommandItem>
						))}
					</CommandGroup>
				))}
				<CommandSeparator />
				<CommandGroup heading="Actions">
					<CommandItem
						value="open settings"
						onSelect={() => onOpenChange(false)}
					>
						Open settings
					</CommandItem>
					<CommandItem
						value="rebuild contracts"
						onSelect={() => onOpenChange(false)}
					>
						Rebuild contracts
						<CommandShortcut>Stub</CommandShortcut>
					</CommandItem>
				</CommandGroup>
			</CommandList>
		</CommandDialog>
	);
}
