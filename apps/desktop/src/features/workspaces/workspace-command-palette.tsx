import { useMemo } from "react";
import { useTranslation } from "react-i18next";
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
	const { t } = useTranslation("common");
	const groupedWorkspaces = useMemo(
		() => [
			{
				label: t("commandPalette.switchWorkspace"),
				items: workspaces,
			},
		],
		[t, workspaces],
	);

	return (
		<CommandDialog
			open={open}
			onOpenChange={onOpenChange}
			title={t("commandPalette.title")}
			description={t("commandPalette.description")}
		>
			<CommandInput placeholder={t("commandPalette.searchPlaceholder")} />
			<CommandList>
				<CommandEmpty>{t("commandPalette.empty")}</CommandEmpty>
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
									<CommandShortcut>{t("commandPalette.selected")}</CommandShortcut>
								) : null}
							</CommandItem>
						))}
					</CommandGroup>
				))}
				<CommandSeparator />
				<CommandGroup heading={t("commandPalette.actions")}>
					<CommandItem
						value="open project"
						onSelect={() => {
							onOpenChange(false);
							onCreateWorkspace();
						}}
					>
						{t("commandPalette.openProject")}
						<CommandShortcut>⌘O</CommandShortcut>
					</CommandItem>
					<CommandItem
						value="clone from url"
						onSelect={() => {
							onOpenChange(false);
							onCloneWorkspace();
						}}
					>
						{t("commandPalette.cloneFromUrl")}
					</CommandItem>
					<CommandItem
						value="open settings"
						onSelect={() => {
							onOpenChange(false);
							onOpenSettings();
						}}
					>
						{t("commandPalette.openSettings")}
						<CommandShortcut>⌘,</CommandShortcut>
					</CommandItem>
					<CommandItem
						value="rebuild contracts"
						onSelect={() => onOpenChange(false)}
					>
						{t("commandPalette.rebuildContracts")}
						<CommandShortcut>{t("commandPalette.stub")}</CommandShortcut>
					</CommandItem>
					<CommandItem
						value="open onboarding"
						onSelect={() => {
							onOpenChange(false);
							onOpenOnboarding();
						}}
					>
						{t("commandPalette.openOnboarding")}
					</CommandItem>
					<CommandItem
						value="keyboard shortcuts"
						onSelect={() => {
							onOpenChange(false);
							onOpenShortcuts();
						}}
					>
						{t("commandPalette.keyboardShortcuts")}
					</CommandItem>
				</CommandGroup>
			</CommandList>
		</CommandDialog>
	);
}
