import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQueryClient } from "@tanstack/react-query";
import {
	CircleUserRound,
	GitBranch,
	Keyboard,
	Moon,
	Package,
	Loader2,
	Server,
	Sparkles,
	TerminalSquare,
	SunMedium,
	Wrench,
	type LucideIcon,
} from "lucide-react";
import type { ForgeCliProvider } from "@dcc/contracts";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { cn } from "@/lib/utils";
import { toast } from "sonner";
import type { DccTheme } from "@/components/theme-provider";
import type { ProviderCatalog } from "@dcc/contracts";
import { ProviderSelectionPanel } from "@/features/providers/provider-selection-panel";
import { ProviderRuntimePanel } from "@/features/providers/provider-runtime-panel";
import { ForgeConnectDialog } from "@/features/settings/forge-connect-dialog";
import { PairedDevicesPanel } from "@/features/settings/paired-devices";
import type { AppUpdateInfo } from "@/features/updater";
import { getOpenPreferredEditorShortcutKeys } from "@/features/shortcuts/shortcut-utils";
import type {
	ProviderRuntimeDraft,
	ProviderRuntimeSettings,
} from "@/features/providers/provider-runtime-settings";
import {
	getDefaultForgeHost,
	normalizeForgeHost,
	setForgeCliSelectedLogin,
} from "@/lib/forge-cli";
import {
	invalidateForgeCliQueries,
	useForgeCliAccounts,
	useForgeCliHosts,
	useForgeCliStatus,
} from "@/features/settings/forge-cli-queries";
import { useForgeCliLoginsHealth } from "@/features/settings/use-forge-cli-logins-health";
import { WORKSPACE_FORGE_CONTEXT_QUERY_KEY } from "@/features/inspector/use-workspace-forge-context";

type SettingsDialogProps = {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	onOpenShortcuts: () => void;
	theme: DccTheme;
	onThemeChange: (theme: DccTheme) => void;
	providerCatalog: ProviderCatalog | null;
	selectedProviderId: string | null;
	onSelectProvider: (providerId: string) => void;
	selectedModelId: string | null;
	onSelectModel: (modelId: string) => void;
	providerRuntimeSettings: ProviderRuntimeSettings;
	onChangeProviderRuntime: (providerId: string, draft: ProviderRuntimeDraft) => void;
	onClearProviderRuntime: (providerId: string) => void;
	appVersion?: string | null;
	appUpdate?: AppUpdateInfo;
	isCheckingUpdate?: boolean;
	isInstallingUpdate?: boolean;
	updateCheckError?: string | null;
	onCheckForUpdate?: () => void;
	onInstallUpdate?: () => void;
};

export type SettingsSectionId =
	| "general"
	| "appearance"
	| "model"
	| "connections"
	| "shortcuts"
	| "git"
	| "experimental"
	| "account";

type SettingsSectionMeta = {
	id: SettingsSectionId;
	label: string;
	description: string;
	icon: LucideIcon;
	status?: "comingSoon";
};

function SectionButton({
	active,
	icon: Icon,
	label,
	description,
	statusLabel,
	onClick,
}: {
	active: boolean;
	icon: LucideIcon;
	label: string;
	description: string;
	statusLabel?: string;
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
			<div className="min-w-0 flex-1">
				<div className="text-[13px] font-medium leading-tight">{label}</div>
				<p className="mt-0.5 text-[11px] leading-tight text-muted-foreground/80">{description}</p>
			</div>
			{statusLabel ? (
				<Badge variant="outline" className="mt-0.5 shrink-0 px-2 text-[10px] font-normal text-muted-foreground">
					{statusLabel}
				</Badge>
			) : null}
		</button>
	);
}

function SectionHeaderBadge({ children }: { children: string }) {
	return (
		<Badge variant="outline" className="h-8 px-3 text-[12px] font-normal text-muted-foreground">
			{children}
		</Badge>
	);
}

function ComingSoonCard({
	title,
	body,
}: {
	title: string;
	body: string;
}) {
	const { t } = useTranslation("common");
	return (
		<div className="rounded-xl border border-border/60 bg-muted/15 p-4">
			<div className="flex items-start justify-between gap-4">
				<div className="min-w-0">
					<h3 className="text-[14px] font-medium text-foreground">{title}</h3>
					<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">{body}</p>
				</div>
				<SectionHeaderBadge>{t("settings.statusComingSoon")}</SectionHeaderBadge>
			</div>
		</div>
	);
}

function ForgeCliIntegrationCard() {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const [provider, setProvider] = useState<ForgeCliProvider>("github");
	const [hosts, setHosts] = useState<Record<ForgeCliProvider, string>>({
		github: getDefaultForgeHost("github"),
		gitlab: getDefaultForgeHost("gitlab"),
	});
	const [connectOpen, setConnectOpen] = useState(false);
	const host = hosts[provider];
	const normalizedHost = normalizeForgeHost(provider, host);
	const accountsQuery = useForgeCliAccounts(provider, normalizedHost);
	const hostsQuery = useForgeCliHosts(provider, { enabled: true });
	const statusQuery = useForgeCliStatus(provider, normalizedHost);
	useForgeCliLoginsHealth(provider, normalizedHost, { enabled: true });
	const discoveredHosts = hostsQuery.data?.hosts ?? [];

	const accounts = accountsQuery.data ?? {
		provider,
		cliName: provider === "github" ? "gh" : "glab",
		hostname: normalizedHost,
		status: "error" as const,
		login: null,
		selectedLogin: null,
		accounts: [],
		message: t("settings.account.loadingError", {
			provider: provider === "github" ? "GitHub" : "GitLab",
		}),
		loginCommand:
			provider === "github"
				? normalizedHost === "github.com"
					? "gh auth login"
					: `gh auth login --hostname ${normalizedHost}`
				: `glab auth login --hostname ${normalizedHost}`,
	};
	const isReady = accounts.status === "ready";
	const selectedLogin = accounts.selectedLogin ?? null;
	const effectiveSelectedLogin =
		selectedLogin && accounts.accounts.some((account) => account.login === selectedLogin)
			? selectedLogin
			: accounts.login ?? accounts.accounts[0]?.login ?? null;

	useEffect(() => {
		if (!isReady) {
			return;
		}
		const nextLogin = effectiveSelectedLogin;
		if (!nextLogin) {
			return;
		}
		if (selectedLogin === nextLogin) {
			return;
		}
		void setForgeCliSelectedLogin(provider, normalizedHost, nextLogin).then(() => {
			void invalidateForgeCliQueries(queryClient, provider, normalizedHost);
			void queryClient.invalidateQueries({
				queryKey: [WORKSPACE_FORGE_CONTEXT_QUERY_KEY],
			});
		});
	}, [effectiveSelectedLogin, isReady, normalizedHost, provider, queryClient, selectedLogin]);

	const handleRefresh = async () => {
		try {
			await Promise.all([accountsQuery.refetch(), hostsQuery.refetch(), statusQuery.refetch()]);
		} catch {
			toast.error(
				t("settings.account.loadingError", {
					provider: provider === "github" ? "GitHub" : "GitLab",
				}),
			);
		}
	};

	return (
		<>
			<div className="rounded-xl border border-border/60 p-4">
				<div className="flex items-start justify-between gap-4">
					<div className="min-w-0">
						<h3 className="text-[14px] font-medium text-foreground">
							{t("settings.account.cardTitle")}
						</h3>
						<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
							{t("settings.account.cardHint")}
						</p>
					</div>
					<Badge
						variant={isReady ? "success" : "outline"}
						className="h-8 px-3 text-[12px] font-normal"
					>
						{isReady
							? t("settings.account.readyBadge")
							: t("settings.account.notReadyBadge")}
					</Badge>
				</div>

				<div className="mt-4 space-y-4">
					<Tabs
						value={provider}
						onValueChange={(value) => setProvider(value as ForgeCliProvider)}
					>
						<TabsList className="w-full">
							<TabsTrigger value="github">GitHub</TabsTrigger>
							<TabsTrigger value="gitlab">GitLab</TabsTrigger>
						</TabsList>
					</Tabs>

					<div className="grid gap-2">
						<label className="text-[12px] font-medium text-foreground">
							{t("settings.account.hostLabel")}
						</label>
						<Input
							value={host}
							onChange={(event) =>
								setHosts((current) => ({
									...current,
									[provider]: event.target.value,
								}))
							}
							placeholder={provider === "github" ? "github.com" : "gitlab.com"}
						/>
						{discoveredHosts.length > 0 ? (
							<div className="flex flex-wrap gap-2">
								{discoveredHosts.map((candidateHost) => {
									const active = candidateHost === normalizedHost;
									return (
										<Button
											key={candidateHost}
											type="button"
											variant={active ? "default" : "outline"}
											size="sm"
											onClick={() =>
												setHosts((current) => ({
													...current,
													[provider]: candidateHost,
												}))
											}
										>
											{candidateHost}
										</Button>
									);
								})}
							</div>
						) : null}
						<p className="text-[11px] leading-relaxed text-muted-foreground">
							{discoveredHosts.length > 0
								? t("settings.account.knownHosts", {
										hosts: discoveredHosts.join(", "),
									})
								: provider === "github"
									? t("settings.account.githubHint")
									: t("settings.account.gitlabHint")}
						</p>
					</div>

					<div className="flex flex-wrap items-center gap-2">
						{accountsQuery.isPending ? (
							<div className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border/60 px-3 text-[12px] text-muted-foreground">
								<Loader2 className="size-3.5 animate-spin" />
								{t("settings.account.checking")}
							</div>
						) : isReady ? (
							<>
								<div className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border/60 px-3 text-[12px] text-foreground">
									<TerminalSquare className="size-3.5" />
									<span className="truncate">
										{accounts.accounts.length > 1
											? t("settings.account.accountsConnected", {
													count: accounts.accounts.length,
													logins: accounts.accounts
														.map((account) => account.login)
														.join(", "),
												})
											: accounts.login ?? accounts.message}
									</span>
								</div>
								<Button variant="ghost" size="sm" onClick={() => void handleRefresh()}>
									{t("settings.account.refresh")}
								</Button>
								<Button variant="outline" size="sm" onClick={() => setConnectOpen(true)}>
									<TerminalSquare className="size-3.5" />
									{t("settings.account.switchAccount")}
								</Button>
							</>
						) : (
							<>
								<Button variant="outline" size="sm" onClick={() => setConnectOpen(true)}>
									<TerminalSquare className="size-3.5" />
									{t("settings.account.connect")}
								</Button>
								<Button variant="ghost" size="sm" onClick={() => void handleRefresh()}>
									{t("settings.account.refresh")}
								</Button>
							</>
						)}
					</div>

					{isReady && accounts.accounts.length > 0 ? (
						<div className="grid gap-2">
							<label className="text-[12px] font-medium text-foreground">
								{t("settings.account.accountLabel")}
							</label>
							<div className="flex flex-wrap gap-2">
								{accounts.accounts.map((account) => {
									const active = account.login === effectiveSelectedLogin;
									const label = account.name
										? `${account.name} · @${account.login}`
										: account.login;
									const title = [account.name, account.email, `@${account.login}`]
										.filter(Boolean)
										.join(" · ");
									return (
										<Button
											key={account.login}
											type="button"
											variant={active ? "default" : "outline"}
											size="sm"
											title={title}
											onClick={() => {
												void setForgeCliSelectedLogin(
													provider,
													normalizedHost,
													account.login,
												).then(() => {
													void invalidateForgeCliQueries(
														queryClient,
														provider,
														normalizedHost,
													);
													void queryClient.invalidateQueries({
														queryKey: [WORKSPACE_FORGE_CONTEXT_QUERY_KEY],
													});
												});
											}}
										>
											{label}
										</Button>
									);
								})}
							</div>
						</div>
					) : null}

					<div className="min-w-0 flex-1">
						<p className="text-[12px] leading-relaxed text-muted-foreground">
							{accounts.message}
						</p>
						<p className="mt-0.5 text-[11px] leading-snug text-muted-foreground/80">
							{t("settings.account.command", {
								command: accounts.loginCommand,
							})}
						</p>
					</div>
				</div>
			</div>
			<ForgeConnectDialog
				open={connectOpen}
				onOpenChange={setConnectOpen}
				provider={provider}
				host={normalizedHost}
				onConnected={() => {
					void invalidateForgeCliQueries(queryClient, provider, normalizedHost);
				}}
			/>
		</>
	);
}

export function SettingsDialog({
	open,
	onOpenChange,
	onOpenShortcuts,
	theme,
	onThemeChange,
	providerCatalog,
	selectedProviderId,
	onSelectProvider,
	selectedModelId,
	onSelectModel,
	providerRuntimeSettings,
	onChangeProviderRuntime,
	onClearProviderRuntime,
	appVersion = null,
	appUpdate = null,
	isCheckingUpdate = false,
	isInstallingUpdate = false,
	updateCheckError = null,
	onCheckForUpdate,
	onInstallUpdate,
}: SettingsDialogProps) {
	const { t, i18n } = useTranslation("common");
	const [activeSection, setActiveSection] = useState<SettingsSectionId>("general");
	const providers = providerCatalog?.providers ?? [];
	const shortcutBadges = useMemo(
		() => ["Cmd/Ctrl+Enter", "Esc", getOpenPreferredEditorShortcutKeys().join("+")],
		[],
	);

	const sections = useMemo<SettingsSectionMeta[]>(
		() => [
			{
				id: "general",
				label: t("settings.sections.general.label"),
				description: t("settings.sections.general.description"),
				icon: Wrench,
			},
			{
				id: "appearance",
				label: t("settings.sections.appearance.label"),
				description: t("settings.sections.appearance.description"),
				icon: SunMedium,
			},
			{
				id: "model",
				label: t("settings.sections.model.label"),
				description: t("settings.sections.model.description"),
				icon: Sparkles,
			},
			{
				id: "connections",
				label: t("settings.sections.connections.label"),
				description: t("settings.sections.connections.description"),
				icon: Server,
			},
			{
				id: "shortcuts",
				label: t("settings.sections.shortcuts.label"),
				description: t("settings.sections.shortcuts.description"),
				icon: Keyboard,
				status: "comingSoon",
			},
			{
				id: "git",
				label: t("settings.sections.git.label"),
				description: t("settings.sections.git.description"),
				icon: GitBranch,
				status: "comingSoon",
			},
			{
				id: "experimental",
				label: t("settings.sections.experimental.label"),
				description: t("settings.sections.experimental.description"),
				icon: Package,
				status: "comingSoon",
			},
			{
				id: "account",
				label: t("settings.sections.account.label"),
				description: t("settings.sections.account.description"),
				icon: CircleUserRound,
			},
		],
		[t],
	);

	useEffect(() => {
		if (open) {
			setActiveSection("general");
		}
	}, [open]);

	const activeMeta = useMemo(
		() => sections.find((section) => section.id === activeSection) ?? sections[0]!,
		[activeSection, sections],
	);

	const uiLocale = i18n.language === "en" ? "en" : "pt-BR";

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="h-[min(84vh,760px)] w-[min(94vw,1240px)] sm:max-w-[1240px] overflow-hidden rounded-2xl border-border/60 bg-background p-0 shadow-2xl">
				<div className="flex h-full min-h-0 w-full min-w-0 overflow-hidden">
					<nav className="scrollbar-stable flex w-[240px] shrink-0 flex-col overflow-x-hidden overflow-y-auto border-r border-sidebar-border bg-sidebar py-5">
						<div className="px-4 pb-3">
							<DialogHeader>
								<DialogTitle className="text-[15px] font-semibold text-foreground">
									{t("settings.title")}
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
									statusLabel={section.status ? t("settings.statusComingSoon") : undefined}
									onClick={() => setActiveSection(section.id)}
								/>
							))}
						</div>

						<div className="mt-auto px-3 pt-5">
							<p className="px-3 pb-2 text-[11px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
								{t("settings.repository")}
							</p>
							<div className="flex w-full items-start gap-3 rounded-lg border border-border/60 bg-background px-3 py-2 text-muted-foreground">
								<GitBranch className="mt-0.5 size-4 shrink-0" strokeWidth={1.9} aria-hidden />
								<div className="min-w-0">
									<div className="text-[13px] font-medium leading-tight">{t("settings.currentWorkspace")}</div>
									<p className="mt-0.5 text-[11px] leading-tight text-muted-foreground/80">
										{t("settings.currentWorkspaceHint")}
									</p>
								</div>
							</div>
						</div>
					</nav>

					<div className="flex min-w-0 flex-1 flex-col overflow-hidden">
						<div className="flex items-center justify-between gap-4 border-b border-border/40 px-6 py-4 lg:px-8">
							<div className="min-w-0">
								<DialogTitle className="text-[15px] font-semibold text-foreground">
									{activeMeta.label}
								</DialogTitle>
								<DialogDescription className="mt-0.5 text-[12px] text-muted-foreground">
									{activeMeta.description}
								</DialogDescription>
							</div>
							{activeMeta.status ? <SectionHeaderBadge>{t("settings.statusComingSoon")}</SectionHeaderBadge> : null}
						</div>

						<div className="min-w-0 flex-1 overflow-x-hidden overflow-y-auto px-6 pt-5 pb-6 lg:px-8">
							{activeSection === "general" ? (
								<section className="space-y-4">
									<div className="rounded-xl border border-border/60 bg-muted/15 p-4">
										<div className="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
											<div className="min-w-0">
												<h3 className="text-[14px] font-medium text-foreground">
													{t("settings.general.appVersionTitle")}
												</h3>
												<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
													{t("settings.general.appVersionBody")}
												</p>
												<p className="mt-3 font-mono text-[13px] text-foreground">
													{appVersion ?? t("settings.general.appVersionUnknown")}
												</p>
											</div>
											<div className="flex shrink-0 flex-col items-stretch gap-2 sm:items-end">
												{appUpdate ? (
													<Badge variant="secondary" className="h-8 px-3 text-[12px] font-normal">
														{t("settings.general.updateAvailable", {
															version: appUpdate.version,
														})}
													</Badge>
												) : (
													<Badge variant="outline" className="h-8 px-3 text-[12px] font-normal">
														{isCheckingUpdate
															? t("settings.general.checkingUpdates")
															: t("settings.general.upToDate")}
													</Badge>
												)}
												<div className="flex flex-wrap gap-2 sm:justify-end">
													<Button
														type="button"
														variant="outline"
														size="sm"
														disabled={isCheckingUpdate || isInstallingUpdate}
														onClick={() => onCheckForUpdate?.()}
													>
														{isCheckingUpdate ? (
															<Loader2 className="size-3.5 animate-spin" />
														) : null}
														{t("settings.general.checkForUpdates")}
													</Button>
													{appUpdate ? (
														<Button
															type="button"
															size="sm"
															disabled={isInstallingUpdate}
															onClick={() => onInstallUpdate?.()}
														>
															{isInstallingUpdate ? (
																<Loader2 className="size-3.5 animate-spin" />
															) : null}
															{isInstallingUpdate
																? t("updater.installing")
																: t("settings.general.installUpdate")}
														</Button>
													) : null}
												</div>
											</div>
										</div>
										{updateCheckError ? (
											<p className="mt-3 text-[12px] leading-relaxed text-destructive">
												{t("settings.general.updateCheckFailed", {
													message: updateCheckError,
												})}
											</p>
										) : null}
									</div>

									<div className="rounded-xl border border-border/60 bg-muted/15 p-4">
										<div className="flex items-start justify-between gap-6">
											<div className="min-w-0">
												<h3 className="text-[14px] font-medium text-foreground">{t("settings.general.shellBehavior")}</h3>
												<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
													{t("settings.general.shellBehaviorBody")}
												</p>
											</div>
											<Badge variant="outline" className="h-8 px-3 text-[12px] font-normal">
												{t("settings.general.shellReady")}
											</Badge>
										</div>
									</div>

									<div className="grid gap-3 sm:grid-cols-2">
										<div className="rounded-xl border border-border/60 p-4">
											<p className="text-[11px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
												{t("settings.general.defaultEntry")}
											</p>
											<p className="mt-2 text-[13px] text-foreground">
												{t("settings.general.defaultEntryBody")}
											</p>
										</div>
										<div className="rounded-xl border border-border/60 p-4">
											<p className="text-[11px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
												{t("settings.general.commandPalette")}
											</p>
											<p className="mt-2 text-[13px] text-foreground">
												{t("settings.general.commandPaletteBody")}
											</p>
										</div>
									</div>
								</section>
							) : null}

							{activeSection === "appearance" ? (
								<section className="space-y-4">
									<div className="flex flex-col gap-4 border-b border-border/40 pb-4">
										<div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between sm:gap-6">
											<div>
												<h3 className="text-[14px] font-medium text-foreground">{t("settings.languageTitle")}</h3>
												<p className="mt-1 text-[12px] text-muted-foreground">{t("settings.languageHint")}</p>
											</div>
											<ToggleGroup
												type="single"
												value={uiLocale}
												onValueChange={(value) => {
													if (value === "pt-BR" || value === "en") {
														void i18n.changeLanguage(value);
													}
												}}
												className="gap-1 self-start sm:self-auto"
											>
												<ToggleGroupItem
													value="pt-BR"
													className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border/60 px-3 text-[12px] font-medium text-muted-foreground data-[state=on]:bg-accent data-[state=on]:text-foreground"
												>
													{t("settings.localePtBr")}
												</ToggleGroupItem>
												<ToggleGroupItem
													value="en"
													className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border/60 px-3 text-[12px] font-medium text-muted-foreground data-[state=on]:bg-accent data-[state=on]:text-foreground"
												>
													{t("settings.localeEn")}
												</ToggleGroupItem>
											</ToggleGroup>
										</div>
									</div>
									<div className="flex items-start justify-between gap-6 border-b border-border/40 pb-4">
										<div>
											<h3 className="text-[14px] font-medium text-foreground">{t("settings.appearance.theme")}</h3>
											<p className="mt-1 text-[12px] text-muted-foreground">
												{t("settings.appearance.themeHint")}
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
												{ value: "light" as const, label: t("settings.appearance.light"), icon: SunMedium },
												{ value: "dark" as const, label: t("settings.appearance.dark"), icon: Moon },
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
											{t("settings.appearance.colorSystemHint")}
										</p>
									</div>
								</section>
							) : null}

							{activeSection === "model" ? (
								<section className="space-y-4">
									<ProviderSelectionPanel
										title={t("settings.model.providersTitle")}
										description={t("settings.model.providersHint")}
										providers={providers}
										selectedProviderId={selectedProviderId}
										selectedModelId={selectedModelId}
										onSelectProvider={onSelectProvider}
										onSelectModel={onSelectModel}
									/>
									<ProviderRuntimePanel
										providers={providers}
										runtimeSettings={providerRuntimeSettings}
										onChangeRuntime={onChangeProviderRuntime}
										onClearRuntime={onClearProviderRuntime}
									/>
								</section>
							) : null}

							{activeSection === "connections" ? (
								<PairedDevicesPanel defaultBackendUrl="http://127.0.0.1:9876" />
							) : null}

							{activeSection === "shortcuts" ? (
								<section className="space-y-4">
									<div className="rounded-xl border border-border/60 p-4">
										<div className="flex items-start justify-between gap-4">
											<div>
												<h3 className="text-[14px] font-medium text-foreground">{t("settings.shortcuts.keyboardShortcuts")}</h3>
												<p className="mt-1 text-[12px] text-muted-foreground">
													{t("settings.shortcuts.keyboardShortcutsHint")}
												</p>
											</div>
											<div className="flex flex-col items-end gap-2">
												<SectionHeaderBadge>{t("settings.statusComingSoon")}</SectionHeaderBadge>
												<Button variant="outline" size="sm" onClick={onOpenShortcuts}>
													{t("settings.shortcuts.openCheatsheet")}
												</Button>
											</div>
										</div>
										<p className="mt-3 text-[12px] leading-relaxed text-muted-foreground">
											{t("settings.shortcuts.comingSoonBody")}
										</p>
										<div className="mt-4 flex flex-wrap gap-2">
											{shortcutBadges.map((shortcut) => (
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
									<ComingSoonCard
										title={t("settings.git.chromeTitle")}
										body={t("settings.git.comingSoonBody")}
									/>
									<div className="rounded-xl border border-border/60 p-4">
										<div className="flex items-start justify-between gap-6">
											<div>
												<h3 className="text-[14px] font-medium text-foreground">{t("settings.git.chromeTitle")}</h3>
												<p className="mt-1 text-[12px] text-muted-foreground">
													{t("settings.git.chromeHint")}
												</p>
											</div>
											<SectionHeaderBadge>{t("settings.git.prState")}</SectionHeaderBadge>
										</div>
										<div className="mt-4 flex flex-wrap gap-2">
											<Badge variant="outline">{t("settings.git.badgeBranchToolbar")}</Badge>
											<Badge variant="outline">{t("settings.git.badgeCommitButton")}</Badge>
											<Badge variant="outline">{t("settings.git.badgeHeaderShimmer")}</Badge>
										</div>
									</div>
								</section>
							) : null}

							{activeSection === "experimental" ? (
								<section className="space-y-4">
									<ComingSoonCard
										title={t("settings.experimental.title")}
										body={t("settings.experimental.comingSoonBody")}
									/>
								</section>
							) : null}

							{activeSection === "account" ? (
								<section className="space-y-4">
									<div className="flex items-start justify-between gap-6 border-b border-border/40 pb-4">
										<div>
											<h3 className="text-[14px] font-medium text-foreground">{t("settings.account.title")}</h3>
											<p className="mt-1 text-[12px] text-muted-foreground">
												{t("settings.account.hint")}
											</p>
										</div>
										<Badge variant="outline" className="h-8 px-3 text-[12px] font-normal">
											{t("settings.account.sectionBadge")}
										</Badge>
									</div>
									<ForgeCliIntegrationCard />
								</section>
							) : null}
						</div>
					</div>
				</div>
			</DialogContent>
		</Dialog>
	);
}
