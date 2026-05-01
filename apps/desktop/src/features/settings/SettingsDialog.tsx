import { useEffect, useMemo, useState } from "react";
import {
	CircleUserRound,
	GitBranch,
	Keyboard,
	Moon,
	Package,
	Sparkles,
	SunMedium,
	Wrench,
	type LucideIcon,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { cn } from "@/lib/utils";
import type { DccTheme } from "@/components/theme-provider";
import type { ProviderCatalog } from "@dcc/contracts";
import { ProviderSelectionPanel } from "@/features/providers/provider-selection-panel";

type SettingsDialogProps = {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	theme: DccTheme;
	onThemeChange: (theme: DccTheme) => void;
	providerCatalog: ProviderCatalog | null;
	selectedProviderId: string | null;
	onSelectProvider: (providerId: string) => void;
	selectedModelId: string | null;
	onSelectModel: (modelId: string) => void;
};

const sections = [
	{
		id: "general",
		label: "General",
		description: "Workspace defaults and shell behavior.",
		icon: Wrench,
	},
	{
		id: "appearance",
		label: "Appearance",
		description: "Theme and chrome contrast.",
		icon: SunMedium,
	},
	{
		id: "model",
		label: "Model",
		description: "Default provider for new sessions.",
		icon: Sparkles,
	},
	{
		id: "shortcuts",
		label: "Shortcuts",
		description: "Keyboard bindings and command palette.",
		icon: Keyboard,
	},
	{
		id: "git",
		label: "Git",
		description: "Branch and PR-state chrome.",
		icon: GitBranch,
	},
	{
		id: "experimental",
		label: "Experimental",
		description: "Preview toggles and future shell flags.",
		icon: Package,
	},
	{
		id: "account",
		label: "Account",
		description: "GitHub and future sync state.",
		icon: CircleUserRound,
	},
] as const;

type SettingsSectionId = (typeof sections)[number]["id"];

function SectionButton({
	active,
	icon: Icon,
	label,
	description,
	onClick,
}: {
	active: boolean;
	icon: LucideIcon;
	label: string;
	description: string;
	onClick: () => void;
}) {
	return (
		<button
			type="button"
			onClick={onClick}
			className={cn(
				"flex w-full items-start gap-3 rounded-lg px-3 py-2 text-left transition-colors",
				active ? "bg-accent/70 text-foreground" : "text-muted-foreground hover:bg-accent/50 hover:text-foreground",
			)}
		>
			<Icon className="mt-0.5 size-4 shrink-0" strokeWidth={1.9} aria-hidden />
			<div className="min-w-0">
				<div className="text-[13px] font-medium leading-tight">{label}</div>
				<p className="mt-0.5 text-[11px] leading-tight text-muted-foreground/80">{description}</p>
			</div>
		</button>
	);
}

export function SettingsDialog({
	open,
	onOpenChange,
	theme,
	onThemeChange,
	providerCatalog,
	selectedProviderId,
	onSelectProvider,
	selectedModelId,
	onSelectModel,
}: SettingsDialogProps) {
	const [activeSection, setActiveSection] = useState<SettingsSectionId>("general");
	const providers = providerCatalog?.providers ?? [];

	useEffect(() => {
		if (open) {
			setActiveSection("general");
		}
	}, [open]);

	const activeMeta = useMemo(
		() => sections.find((section) => section.id === activeSection) ?? sections[0]!,
		[activeSection],
	);

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="h-[min(80vh,640px)] w-[min(80vw,860px)] max-w-[860px] overflow-hidden rounded-2xl border-border/60 bg-background p-0 shadow-2xl">
				<div className="flex h-full min-h-0 w-full min-w-0 overflow-hidden">
					<nav className="scrollbar-stable flex w-[220px] shrink-0 flex-col overflow-x-hidden overflow-y-auto border-r border-sidebar-border bg-sidebar py-5">
						<div className="px-4 pb-3">
							<DialogHeader>
								<DialogTitle className="text-[15px] font-semibold text-foreground">
									Settings
								</DialogTitle>
							</DialogHeader>
						</div>

						<div className="space-y-1 px-3">
							{sections.map((section) => (
								<SectionButton
									key={section.id}
									active={section.id === activeSection}
									icon={section.icon}
									label={section.label}
									description={section.description}
									onClick={() => setActiveSection(section.id)}
								/>
							))}
						</div>

						<div className="mt-auto px-3 pt-5">
							<p className="px-3 pb-2 text-[11px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
								Repository
							</p>
							<button
								type="button"
								className="flex w-full items-start gap-3 rounded-lg px-3 py-2 text-left text-muted-foreground hover:bg-accent/50 hover:text-foreground"
							>
								<GitBranch className="mt-0.5 size-4 shrink-0" strokeWidth={1.9} aria-hidden />
								<div className="min-w-0">
									<div className="text-[13px] font-medium leading-tight">Current workspace</div>
									<p className="mt-0.5 text-[11px] leading-tight text-muted-foreground/80">
										Repository-scoped preferences live with the active workspace.
									</p>
								</div>
							</button>
						</div>
					</nav>

					<div className="flex min-w-0 flex-1 flex-col overflow-hidden">
						<div className="flex items-center justify-between gap-4 border-b border-border/40 px-8 py-4">
							<div className="min-w-0">
								<DialogTitle className="text-[15px] font-semibold text-foreground">
									{activeMeta.label}
								</DialogTitle>
								<p className="mt-0.5 text-[12px] text-muted-foreground">
									{activeMeta.description}
								</p>
							</div>
							<Badge variant="outline" className="h-8 px-3 text-[12px] font-normal">
								Phase 3
							</Badge>
						</div>

						<div className="min-w-0 flex-1 overflow-x-hidden overflow-y-auto px-8 pt-5 pb-6">
							{activeSection === "general" ? (
								<section className="space-y-4">
									<div className="rounded-xl border border-border/60 bg-muted/15 p-4">
										<div className="flex items-start justify-between gap-6">
											<div className="min-w-0">
												<h3 className="text-[14px] font-medium text-foreground">Shell behavior</h3>
												<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
													The desktop shell keeps workspace chrome, the thread composer, and the inspector aligned with the blueprint.
												</p>
											</div>
											<Badge variant="outline" className="h-8 px-3 text-[12px] font-normal">
												Shell ready
											</Badge>
										</div>
									</div>

									<div className="grid gap-3 sm:grid-cols-2">
										<div className="rounded-xl border border-border/60 p-4">
											<p className="text-[11px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
												Default entry
											</p>
											<p className="mt-2 text-[13px] text-foreground">
												Workspaces open into the inspector-first workbench with the composer in the center pane.
											</p>
										</div>
										<div className="rounded-xl border border-border/60 p-4">
											<p className="text-[11px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
												Command palette
											</p>
											<p className="mt-2 text-[13px] text-foreground">
												`Cmd/Ctrl+K` opens workspace switching and actions.
											</p>
										</div>
									</div>
								</section>
							) : null}

							{activeSection === "appearance" ? (
								<section className="space-y-4">
									<div className="flex items-start justify-between gap-6 border-b border-border/40 pb-4">
										<div>
											<h3 className="text-[14px] font-medium text-foreground">Theme</h3>
											<p className="mt-1 text-[12px] text-muted-foreground">
												Match the shell to the system or force a fixed mode.
											</p>
										</div>
										<ToggleGroup
											type="single"
											value={theme}
											onValueChange={(value) => {
												if (value === "light" || value === "dark") {
													onThemeChange(value);
												}
											}}
											className="gap-1"
										>
											{[
												{ value: "light" as const, label: "Light", icon: SunMedium },
												{ value: "dark" as const, label: "Dark", icon: Moon },
											].map(({ value, label, icon: Icon }) => (
												<ToggleGroupItem
													key={value}
													value={value}
													className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border/60 px-3 text-[12px] font-medium text-muted-foreground data-[state=on]:bg-accent data-[state=on]:text-foreground"
												>
													<Icon className="size-3.5" strokeWidth={1.8} aria-hidden />
													{label}
												</ToggleGroupItem>
											))}
										</ToggleGroup>
									</div>
									<div className="rounded-xl border border-border/60 p-4">
										<p className="text-[12px] leading-relaxed text-muted-foreground">
											The color system is driven by shell tokens, so this choice changes the whole chrome instead of just the canvas.
										</p>
									</div>
								</section>
							) : null}

							{activeSection === "model" ? (
								<section className="space-y-4">
									<ProviderSelectionPanel
										title="Providers"
										description="Choose the default runtime for new sessions and composer sends."
										providers={providers}
										selectedProviderId={selectedProviderId}
										selectedModelId={selectedModelId}
										onSelectProvider={onSelectProvider}
										onSelectModel={onSelectModel}
									/>
								</section>
							) : null}

							{activeSection === "shortcuts" ? (
								<section className="space-y-4">
									<div className="rounded-xl border border-border/60 p-4">
										<div className="flex items-center justify-between gap-4">
											<div>
												<h3 className="text-[14px] font-medium text-foreground">Keyboard shortcuts</h3>
												<p className="mt-1 text-[12px] text-muted-foreground">
													Command palette and composer shortcuts already wired in this shell.
												</p>
											</div>
											<Badge variant="outline" className="h-8 px-3 text-[12px] font-normal">
												3 active
											</Badge>
										</div>
										<div className="mt-4 flex flex-wrap gap-2">
											{["Cmd/Ctrl+K", "Cmd/Ctrl+Enter", "Esc"].map((shortcut) => (
												<Badge key={shortcut} variant="secondary" className="h-8 px-3 text-[12px] font-normal">
													{shortcut}
												</Badge>
											))}
										</div>
									</div>
								</section>
							) : null}

							{activeSection === "git" ? (
								<section className="space-y-4">
									<div className="rounded-xl border border-border/60 p-4">
										<div className="flex items-start justify-between gap-6">
											<div>
												<h3 className="text-[14px] font-medium text-foreground">Git chrome</h3>
												<p className="mt-1 text-[12px] text-muted-foreground">
													Branch-aware header styling and the commit action live in the inspector.
												</p>
											</div>
											<Badge variant="outline" className="h-8 px-3 text-[12px] font-normal">
												PR state
											</Badge>
										</div>
										<div className="mt-4 flex flex-wrap gap-2">
											<Badge variant="outline">Branch toolbar</Badge>
											<Badge variant="outline">Commit button</Badge>
											<Badge variant="outline">Header shimmer</Badge>
										</div>
									</div>
								</section>
							) : null}

							{activeSection === "experimental" ? (
								<section className="space-y-4">
									<div className="rounded-xl border border-border/60 p-4">
										<h3 className="text-[14px] font-medium text-foreground">Experimental</h3>
										<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
											No preview toggles are exposed yet. This section stays in the nav so future shell flags land without changing the layout.
										</p>
									</div>
								</section>
							) : null}

							{activeSection === "account" ? (
								<section className="space-y-4">
									<div className="flex items-start justify-between gap-6 border-b border-border/40 pb-4">
										<div>
											<h3 className="text-[14px] font-medium text-foreground">GitHub account</h3>
											<p className="mt-1 text-[12px] text-muted-foreground">
												Account and sync surfaces are gated behind the future integration pass.
											</p>
										</div>
										<Badge variant="outline" className="h-8 px-3 text-[12px] font-normal">
											Disconnected
										</Badge>
									</div>
									<div className="rounded-xl border border-border/60 p-4">
										<p className="text-[12px] leading-relaxed text-muted-foreground">
											The settings dialog already reserves the account section so the footer integration can slot in later without reopening the shell contract.
										</p>
									</div>
								</section>
							) : null}
						</div>
					</div>
				</div>
			</DialogContent>
		</Dialog>
	);
}
