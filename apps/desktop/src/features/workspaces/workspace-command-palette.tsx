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
import type { WorkbenchCommand } from "./workbench-command";

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
	onOpenSkills: () => void;
	/** Present only when the active workspace has a session to delegate from. */
	onDelegate?: () => void;
	onRunWorkbenchCommand?: (command: WorkbenchCommand) => void;
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
	onOpenSkills,
	onDelegate,
	onRunWorkbenchCommand,
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
	const workbenchCommands = useMemo<Array<{ command: WorkbenchCommand; label: string; keywords: string }>>(
		() => [
			{ command: "composer.focus", label: t("commandPalette.workbench.focusComposer"), keywords: "composer prompt focus" },
			{ command: "composer.addContext", label: t("commandPalette.workbench.addContext"), keywords: "composer context directory" },
			{ command: "composer.execution", label: t("commandPalette.workbench.execution"), keywords: "composer effort fast ultrathink" },
			{ command: "composer.togglePlan", label: t("commandPalette.workbench.togglePlan"), keywords: "composer plan mode" },
			{ command: "terminal.openWorktree", label: t("commandPalette.workbench.terminalWorktree"), keywords: "terminal worktree" },
			{ command: "terminal.openProject", label: t("commandPalette.workbench.terminalProject"), keywords: "terminal project root" },
			{ command: "terminal.newWorktree", label: t("commandPalette.workbench.newTerminal"), keywords: "terminal new tab" },
			{ command: "inspector.changes", label: t("commandPalette.workbench.inspectorChanges"), keywords: "inspector git changes diff" },
			{ command: "inspector.files", label: t("commandPalette.workbench.inspectorFiles"), keywords: "inspector files code" },
			{ command: "inspector.activity", label: t("commandPalette.workbench.inspectorActivity"), keywords: "inspector activity milestones" },
			{ command: "inspector.details", label: t("commandPalette.workbench.inspectorDetails"), keywords: "inspector details runtime diagnostics" },
		],
		[t],
	);

	return (
		<CommandDialog
			open={open}
			onOpenChange={onOpenChange}
			title={t("commandPalette.title")}
			description={t("commandPalette.description")}
		>
			<div className="border-b border-border/60 bg-muted/20 px-4 py-3">
				<p className="text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
					{t("commandPalette.title")}
				</p>
				<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
					{t("commandPalette.description")}
				</p>
			</div>
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
				{selectedWorkspaceId && onRunWorkbenchCommand ? (
					<>
						<CommandGroup heading={t("commandPalette.workbench.title")}>
							{workbenchCommands.map((item) => (
								<CommandItem
									key={item.command}
									value={`${item.label} ${item.keywords}`}
									onSelect={() => {
										onOpenChange(false);
										onRunWorkbenchCommand(item.command);
									}}
								>
									{item.label}
								</CommandItem>
							))}
						</CommandGroup>
						<CommandSeparator />
					</>
				) : null}
				<CommandGroup heading={t("commandPalette.actions")}>
					{onDelegate ? (
						<CommandItem
							value="delegate agent delegation"
							onSelect={() => {
								onOpenChange(false);
								onDelegate();
							}}
						>
							{t("commandPalette.delegate")}
						</CommandItem>
					) : null}
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
						value="manage skills"
						onSelect={() => {
							onOpenChange(false);
							onOpenSkills();
						}}
					>
						{t("commandPalette.manageSkills")}
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
