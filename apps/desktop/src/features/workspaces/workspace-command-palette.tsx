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
	selectedWorkspaceId: string | null;
	onSelectWorkspace: (workspaceId: string) => void;
	onCreateWorkspace: () => void;
	onCloneWorkspace: () => void;
	onOpenSettings: () => void;
	onOpenOnboarding: () => void;
	onOpenShortcuts: () => void;
};

export function WorkspaceCommandPalette({
	open,
	onOpenChange,
	workspaces,
	selectedWorkspaceId,
	onSelectWorkspace,
	onCreateWorkspace,
	onCloneWorkspace,
	onOpenSettings,
	onOpenOnboarding,
	onOpenShortcuts,
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
			title="Workspace command palette"
			description="Jump to a workspace or run an action."
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
						value="open project"
						onSelect={() => {
							onOpenChange(false);
							onCreateWorkspace();
						}}
					>
						Open project
						<CommandShortcut>⌘O</CommandShortcut>
					</CommandItem>
					<CommandItem
						value="clone from url"
						onSelect={() => {
							onOpenChange(false);
							onCloneWorkspace();
						}}
					>
						Clone from URL
					</CommandItem>
					<CommandItem
						value="open settings"
						onSelect={() => {
							onOpenChange(false);
							onOpenSettings();
						}}
					>
						Open settings
						<CommandShortcut>⌘,</CommandShortcut>
					</CommandItem>
					<CommandItem
						value="rebuild contracts"
						onSelect={() => onOpenChange(false)}
					>
						Rebuild contracts
						<CommandShortcut>Stub</CommandShortcut>
					</CommandItem>
					<CommandItem
						value="open onboarding"
						onSelect={() => {
							onOpenChange(false);
							onOpenOnboarding();
						}}
					>
						Open onboarding
					</CommandItem>
					<CommandItem
						value="keyboard shortcuts"
						onSelect={() => {
							onOpenChange(false);
							onOpenShortcuts();
						}}
					>
						Keyboard shortcuts
					</CommandItem>
				</CommandGroup>
			</CommandList>
		</CommandDialog>
	);
}
