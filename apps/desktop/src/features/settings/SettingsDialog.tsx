import { FrontendDiagnostics } from "@/components/FrontendDiagnostics";
import { useEffect, useId, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
	Cable,
	CircleUserRound,
	GitBranch,
	Keyboard,
	ListChecks,
	MonitorCog,
	Package,
	Rabbit,
	Loader2,
	Server,
	Sparkles,
	TerminalSquare,
	SunMedium,
	Wrench,
	Database,
	BrainCircuit,
	RefreshCw,
	Copy,
	CircleCheck,
	CircleAlert,
} from "lucide-react";
import type { ForgeCliProvider } from "@dcc/contracts";
import type { AiMemorySettingsInput, DecisionProviderSettingsInput } from "@dcc/contracts";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";
import { toast } from "sonner";
import type { DccDensity, DccTheme } from "@/components/theme-provider";
import type { ProviderCatalog } from "@dcc/contracts";
import { ProviderSelectionPanel } from "@/features/providers/provider-selection-panel";
import { ProviderRuntimePanel } from "@/features/providers/provider-runtime-panel";
import { ProviderAccountUsagePanel } from "@/features/providers/provider-account-usage-panel";
import { CodeRabbitConnectDialog } from "@/features/settings/coderabbit-connect-dialog";
import {
	setCodeRabbitIntegrationEnabled,
	useCodeRabbitIntegrationEnabled,
} from "@/features/settings/coderabbit-preferences";
import {
	invalidateCodeRabbitCliQueries,
	useCodeRabbitCliStatus,
} from "@/features/settings/coderabbit-cli-queries";
import { ForgeConnectDialog } from "@/features/settings/forge-connect-dialog";
import { PairedDevicesPanel } from "@/features/settings/paired-devices";
import type { AppUpdateInfo } from "@/features/updater";
import {
	getCommandPaletteShortcutKeys,
	getFocusComposerShortcutKeys,
	getOpenPreferredEditorShortcutKeys,
	getToggleTerminalShortcutKeys,
} from "@/features/shortcuts/shortcut-utils";
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
import {
	DCC_UX_METRICS_STORAGE_KEY,
	readUxMetrics,
	type DccUxMetricName,
} from "@/lib/ux-metrics";
import { WorkspaceProjectAutomationDialog } from "@/features/automation/workspace-project-automation-dialog";
import { WORKSPACE_GIT_STATUS_QUERY_KEY } from "@/features/inspector/use-workspace-git-status";
import { WORKSPACE_GIT_BRANCH_DIFF_QUERY_KEY } from "@/features/inspector/use-workspace-git-branch-diff";
import { disconnectCodeRabbitCli } from "@/lib/coderabbit-cli";
import { McpIntegrationsPanel } from "@/features/settings/mcp-integrations-panel";
import { ComputerUsePanel } from "@/features/settings/computer-use-panel";
import { AppshotsShortcut } from "@/features/settings/appshots-shortcut";
import { ProviderAvailabilityPanel } from "@/features/providers/provider-availability-panel";
import {
	isProviderAvailabilityRequestCurrent,
	persistProviderAvailability,
} from "@/features/providers/provider-availability.logic";
import { setProviderAvailability } from "@/lib/provider-api";
import {
	loadAiMemoryOutbox,
	loadAiMemoryExportHistory,
	loadAiMemorySettings,
	loadAiMemorySidecarStatus,
	loadDecisionProviderSettings,
	loadDecisionProviderHistory,
	restartDcc,
	retryAiMemoryOutbox,
	saveAiMemorySettings,
	saveDecisionProviderSettings,
} from "@/lib/session-api";
import {
	SettingsNavigation,
	type SettingsSectionId,
	type SettingsSectionMeta,
} from "./settings-navigation";
import { SettingsThemePicker } from "./settings-theme-picker";
import "./settings.css";
export type { SettingsSectionId } from "./settings-navigation";

function formatAiMemoryTimestamp(value: string, language: string) {
	const parsed = new Date(value);
	if (Number.isNaN(parsed.getTime())) {
		return value;
	}

	return new Intl.DateTimeFormat(language === "en" || language.startsWith("en-") ? "en-US" : "pt-BR", {
		dateStyle: "medium",
		timeStyle: "short",
	}).format(parsed);
}

type SettingsDialogProps = {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	onOpenShortcuts: () => void;
	theme: DccTheme;
	onThemeChange: (theme: DccTheme) => void;
	density: DccDensity;
	onDensityChange: (density: DccDensity) => void;
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
	workspaceRoot: string | null;
	workspaceName: string | null;
	projectId: string | null;
	sessionId: string | null;
	sessionProviderId: string | null;
	sessionCreatedAt: string | null;
};

function forgeAccountInitials(value: string): string {
	return (
		value
			.split(/[\s@._/-]+/)
			.map((part) => part.trim())
			.filter(Boolean)
			.slice(0, 2)
			.map((part) => part[0]?.toUpperCase() ?? "")
			.join("") || "FG"
	);
}

function ForgeAccountAvatar({
	avatarUrl,
	label,
}: {
	avatarUrl?: string | null;
	label: string;
}) {
	const [failed, setFailed] = useState(false);
	const className =
		"size-4 shrink-0 overflow-hidden rounded-full border border-border/60 bg-background text-[8px] font-semibold uppercase text-foreground";

	if (avatarUrl && !failed) {
		return (
			<img
				src={avatarUrl}
				alt=""
				aria-hidden
				className={cn(className, "object-cover")}
				onError={() => setFailed(true)}
				referrerPolicy="no-referrer"
			/>
		);
	}

	return (
		<span aria-hidden className={cn(className, "flex items-center justify-center")}>
			{forgeAccountInitials(label)}
		</span>
	);
}

function SectionHeaderBadge({ children }: { children: string }) {
	return (
		<Badge variant="outline" className="h-8 px-3 text-[12px] font-normal text-muted-foreground">
			{children}
		</Badge>
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
											className="gap-2"
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
											<ForgeAccountAvatar
												avatarUrl={account.avatarUrl}
												label={label}
											/>
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

function CodeRabbitCliIntegrationCard() {
	const { t } = useTranslation("common");
	const queryClient = useQueryClient();
	const [connectOpen, setConnectOpen] = useState(false);
	const [isDisconnecting, setIsDisconnecting] = useState(false);
	const integrationEnabled = useCodeRabbitIntegrationEnabled();
	const statusQuery = useCodeRabbitCliStatus(null, { includeAuthStatus: true });
	const status = statusQuery.data;
	const authReady = Boolean(status?.auth?.success || status?.auth?.authenticated);
	const installed = status?.installed ?? false;
	const isReady = installed && authReady;
	const statusMessage = status?.message ?? t("settings.codeRabbit.checkingStatus");
	const authMessage = status?.auth?.message ?? status?.auth?.stderr ?? null;
	const accountLabel =
		status?.auth?.login ||
		status?.auth?.organization ||
		(authReady ? t("settings.codeRabbit.authenticated") : null);

	const handleRefresh = async () => {
		await invalidateCodeRabbitCliQueries(queryClient, null);
	};

	const handleDisconnect = async () => {
		if (!window.confirm(t("settings.codeRabbit.disconnectConfirm"))) {
			return;
		}
		setIsDisconnecting(true);
		try {
			const result = await disconnectCodeRabbitCli(status?.cliPath);
			if (!result.success) {
				throw new Error(result.message);
			}
			setCodeRabbitIntegrationEnabled(false);
			await invalidateCodeRabbitCliQueries(queryClient, null);
			toast.success(t("settings.codeRabbit.disconnectedToast"));
		} catch (error) {
			toast.error(
				error instanceof Error
					? error.message
					: t("settings.codeRabbit.disconnectFailed"),
			);
		} finally {
			setIsDisconnecting(false);
		}
	};

	return (
		<>
			<div className="rounded-xl border border-border/60 p-4">
				<div className="flex flex-col gap-4">
					<div className="flex flex-wrap items-start justify-between gap-3">
						<div className="min-w-0">
							<div className="flex items-center gap-2">
								<Rabbit className="size-4 text-muted-foreground" strokeWidth={1.8} />
								<h3 className="text-[14px] font-medium text-foreground">
									{t("settings.codeRabbit.title")}
								</h3>
								<Badge
									variant={integrationEnabled && isReady ? "success" : "outline"}
									className="h-5 text-[10px] font-normal"
								>
									{!integrationEnabled
										? t("settings.codeRabbit.disabledBadge")
										: isReady
											? t("settings.codeRabbit.readyBadge")
											: t("settings.codeRabbit.notReadyBadge")}
								</Badge>
							</div>
							<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
								{t("settings.codeRabbit.hint")}
							</p>
						</div>

						<div className="flex flex-wrap items-center gap-2">
							{statusQuery.isPending ? (
								<div className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border/60 px-3 text-[12px] text-muted-foreground">
									<Loader2 className="size-3.5 animate-spin" />
									{t("settings.codeRabbit.checking")}
								</div>
							) : isReady ? (
								<>
									<div className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border/60 px-3 text-[12px] text-foreground">
										<TerminalSquare className="size-3.5" />
										<span className="truncate">
											{accountLabel ?? status?.cliName ?? "CodeRabbit"}
										</span>
									</div>
									<Button variant="ghost" size="sm" onClick={() => void handleRefresh()}>
										{t("settings.codeRabbit.refresh")}
									</Button>
									<Button variant="outline" size="sm" onClick={() => setConnectOpen(true)}>
										<TerminalSquare className="size-3.5" />
										{t("settings.codeRabbit.reconnect")}
									</Button>
									<Button
										variant="ghost"
										size="sm"
										disabled={isDisconnecting}
										onClick={() => void handleDisconnect()}
										className="text-destructive hover:text-destructive"
									>
										{isDisconnecting
											? t("settings.codeRabbit.disconnecting")
											: t("settings.codeRabbit.disconnect")}
									</Button>
								</>
							) : (
								<>
									<Button variant="outline" size="sm" onClick={() => setConnectOpen(true)}>
										<TerminalSquare className="size-3.5" />
										{t("settings.codeRabbit.connect")}
									</Button>
									<Button variant="ghost" size="sm" onClick={() => void handleRefresh()}>
										{t("settings.codeRabbit.refresh")}
									</Button>
								</>
							)}
						</div>
					</div>

					<div className="flex items-center justify-between gap-4 rounded-lg border border-border/50 bg-background/50 px-3 py-2.5">
						<div className="min-w-0">
							<div className="text-[12px] font-medium text-foreground">
								{t("settings.codeRabbit.inspectorToggle")}
							</div>
							<p className="mt-0.5 text-[11px] leading-snug text-muted-foreground">
								{t("settings.codeRabbit.inspectorToggleHint")}
							</p>
						</div>
						<Switch
							checked={integrationEnabled}
							onCheckedChange={setCodeRabbitIntegrationEnabled}
							aria-label={t("settings.codeRabbit.inspectorToggle")}
						/>
					</div>

					<div className="min-w-0">
						<p className="text-[12px] leading-relaxed text-muted-foreground">
							{authMessage ?? statusMessage}
						</p>
						<p className="mt-0.5 text-[11px] leading-snug text-muted-foreground/80">
							{t("settings.codeRabbit.command", {
								command: status?.loginCommand ?? "cr auth login",
							})}
						</p>
						{status?.version ? (
							<p className="mt-0.5 text-[11px] leading-snug text-muted-foreground/80">
								{t("settings.codeRabbit.version", { version: status.version })}
							</p>
						) : null}
					</div>
				</div>
			</div>
			<CodeRabbitConnectDialog
				open={connectOpen}
				onOpenChange={setConnectOpen}
				onConnected={() => {
					void invalidateCodeRabbitCliQueries(queryClient, null);
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
	density,
	onDensityChange,
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
	workspaceRoot,
	workspaceName,
	projectId,
	sessionId,
	sessionProviderId,
	sessionCreatedAt,
}: SettingsDialogProps) {
	const { t, i18n } = useTranslation("common");
	const queryClient = useQueryClient();
	const [aiMemoryTab, setAiMemoryTab] = useState("configuration");
	useEffect(() => {
		if (!open) setAiMemoryTab("configuration");
	}, [open]);
	const aiMemoryStatusQuery = useQuery({
		queryKey: ["ai-memory", "sidecar-status"],
		queryFn: loadAiMemorySidecarStatus,
		enabled: open,
		staleTime: 10_000,
	});
	const aiMemorySettingsQuery = useQuery({
		queryKey: ["ai-memory", "settings"],
		queryFn: loadAiMemorySettings,
		enabled: open,
		staleTime: 10_000,
	});
	const aiMemoryOutboxQuery = useQuery({
		queryKey: ["ai-memory", "outbox"],
		queryFn: () => loadAiMemoryOutbox(50),
		enabled: open && aiMemoryTab === "history",
		staleTime: 5_000,
	});
	const aiMemoryExportHistoryQuery = useQuery({
		queryKey: ["ai-memory", "export-history"],
		queryFn: () => loadAiMemoryExportHistory(100),
		enabled: open && aiMemoryTab === "history",
		staleTime: 5_000,
	});
	const [aiMemoryDraft, setAiMemoryDraft] = useState<AiMemorySettingsInput>({
		mode: "managed",
		baseUrl: null,
		workspace: "dcc-workspace",
		project: "dcc-project",
		dataDir: null,
		token: null,
	});
	const [aiMemorySaving, setAiMemorySaving] = useState(false);
	const [aiMemoryRestartRequired, setAiMemoryRestartRequired] = useState(false);
	const [aiMemoryRetrying, setAiMemoryRetrying] = useState<string | null>(null);
	const [decisionProviderTab, setDecisionProviderTab] = useState("configuration");
	useEffect(() => {
		if (!open) setDecisionProviderTab("configuration");
	}, [open]);
	const decisionProviderSettingsQuery = useQuery({
		queryKey: ["decision-provider", "settings"],
		queryFn: loadDecisionProviderSettings,
		enabled: open,
		staleTime: 10_000,
	});
	const decisionProviderHistoryQuery = useQuery({
		queryKey: ["decision-provider", "history"],
		queryFn: () => loadDecisionProviderHistory(100),
		enabled: open && decisionProviderTab === "history",
		staleTime: 5_000,
		refetchInterval: open && decisionProviderTab === "history" ? 10_000 : false,
	});
	const [decisionProviderDraft, setDecisionProviderDraft] = useState<DecisionProviderSettingsInput>({
		provider: "disabled",
		mode: "observe",
		modelRouting: "manual",
		memoryFilterEnabled: true,
		skillRouterEnabled: true,
		modelRouterEnabled: true,
		completionReviewEnabled: true,
		baseUrl: "https://api.typesafe.ai",
		model: "jev-latest",
		memoryThreshold: 0.65,
		skillConfidenceThreshold: 0.65,
		modelConfidenceThreshold: 0.65,
		completionThreshold: 0.65,
		apiKey: null,
		clearApiKey: false,
	});
	const [decisionProviderSaving, setDecisionProviderSaving] = useState(false);
	useEffect(() => {
		const settings = decisionProviderSettingsQuery.data;
		if (!settings) return;
		setDecisionProviderDraft((current) => ({
			...current,
			provider: settings.provider,
			mode: settings.mode,
			modelRouting: settings.modelRouting,
			memoryFilterEnabled: settings.memoryFilterEnabled,
			skillRouterEnabled: settings.skillRouterEnabled,
			modelRouterEnabled: settings.modelRouterEnabled,
			completionReviewEnabled: settings.completionReviewEnabled,
			baseUrl: settings.baseUrl,
			model: settings.model,
			memoryThreshold: settings.memoryThreshold,
			skillConfidenceThreshold: settings.skillConfidenceThreshold,
			modelConfidenceThreshold: settings.modelConfidenceThreshold,
			completionThreshold: settings.completionThreshold,
			apiKey: null,
			clearApiKey: false,
		}));
	}, [decisionProviderSettingsQuery.data]);
	useEffect(() => {
		const settings = aiMemorySettingsQuery.data;
		if (!settings) return;
		setAiMemoryDraft((current) => ({
			...current,
			mode: settings.mode,
			baseUrl: settings.baseUrl,
			workspace: settings.workspace,
			project: settings.project,
			dataDir: settings.dataDir,
			token: null,
		}));
	}, [aiMemorySettingsQuery.data]);
	const [activeSection, setActiveSection] = useState<SettingsSectionId>("general");
	const panelId = useId();
	const headingId = useId();
	const contentRef = useRef<HTMLDivElement>(null);
	const returnFocusRef = useRef<HTMLElement | null>(null);
	useEffect(() => {
		if (contentRef.current) contentRef.current.scrollTop = 0;
	}, [activeSection, open]);
	const [automationOpen, setAutomationOpen] = useState(false);
	const [uxMetricsVersion, setUxMetricsVersion] = useState(0);
	const [pendingAvailabilityProviderIds, setPendingAvailabilityProviderIds] =
		useState<Set<string>>(() => new Set());
	const [availabilityErrors, setAvailabilityErrors] = useState<Record<string, string>>(
		{},
	);
	const mountedRef = useRef(true);
	const openRef = useRef(open);
	const availabilityGenerationRef = useRef(0);
	const availabilityRequestIdsRef = useRef(new Map<string, number>());
	openRef.current = open;

	useEffect(() => {
		availabilityGenerationRef.current += 1;
		availabilityRequestIdsRef.current.clear();
		if (!open) {
			setPendingAvailabilityProviderIds(new Set());
			setAvailabilityErrors({});
		}
	}, [open]);

	useEffect(() => {
		return () => {
			mountedRef.current = false;
			availabilityGenerationRef.current += 1;
			availabilityRequestIdsRef.current.clear();
		};
	}, []);
	const providers = providerCatalog?.providers ?? [];
	const handleProviderAvailabilityChange = async (
		providerId: string,
		enabled: boolean,
	) => {
		if (!openRef.current || !mountedRef.current) {
			return;
		}
		const generation = availabilityGenerationRef.current;
		const requestId =
			(availabilityRequestIdsRef.current.get(providerId) ?? 0) + 1;
		availabilityRequestIdsRef.current.set(providerId, requestId);
		const requestToken = { generation, requestId };
		const isCurrentRequest = () =>
			isProviderAvailabilityRequestCurrent(requestToken, {
				generation: availabilityGenerationRef.current,
				requestId: availabilityRequestIdsRef.current.get(providerId),
				mounted: mountedRef.current,
				open: openRef.current,
			});
		const providerLabel =
			providers.find((provider) => provider.id === providerId)?.label ?? providerId;
		setPendingAvailabilityProviderIds((current) => {
			const next = new Set(current);
			next.add(providerId);
			return next;
		});
		setAvailabilityErrors((current) => {
			const next = { ...current };
			delete next[providerId];
			return next;
		});
		try {
			await persistProviderAvailability(
				{ providerId, enabled },
				{
					setAvailability: setProviderAvailability,
					invalidateCatalog: () =>
						queryClient.invalidateQueries({ queryKey: ["providers", "catalog"] }),
				},
			);
			if (isCurrentRequest()) {
				toast.success(
					enabled
						? t("settings.model.enabledToast", { provider: providerLabel })
						: t("settings.model.disabledToast", { provider: providerLabel }),
				);
			}
		} catch (error) {
			if (!isCurrentRequest()) {
				return;
			}
			const message =
				error instanceof Error
					? error.message
					: t("settings.model.availabilityError");
			setAvailabilityErrors((current) => ({ ...current, [providerId]: message }));
			toast.error(message);
		} finally {
			if (isCurrentRequest()) {
				availabilityRequestIdsRef.current.delete(providerId);
				setPendingAvailabilityProviderIds((current) => {
					const next = new Set(current);
					next.delete(providerId);
					return next;
				});
			}
		}
	};
	const shortcutBadges = useMemo(
		() => [
			getCommandPaletteShortcutKeys().join("+"),
			getFocusComposerShortcutKeys().join("+"),
			getToggleTerminalShortcutKeys().join("+"),
			getOpenPreferredEditorShortcutKeys().join("+"),
			"Esc",
		],
		[],
	);
	const handleAiMemorySettingsSave = async () => {
		setAiMemorySaving(true);
		try {
			const saved = await saveAiMemorySettings(aiMemoryDraft);
			await Promise.all([
				aiMemorySettingsQuery.refetch(),
				aiMemoryStatusQuery.refetch(),
			]);
			toast.success(
				saved.restartRequired
					? t("settings.aiMemory.savedRestart")
					: t("settings.aiMemory.saved"),
			);
			setAiMemoryRestartRequired(saved.restartRequired);
		} catch (error) {
			toast.error(error instanceof Error ? error.message : t("settings.aiMemory.saveError"));
		} finally {
			setAiMemorySaving(false);
		}
	};
	const handleAiMemoryRetry = async (sessionId: string) => {
		setAiMemoryRetrying(sessionId);
		try {
			await retryAiMemoryOutbox(sessionId);
			await Promise.all([aiMemoryOutboxQuery.refetch(), aiMemoryExportHistoryQuery.refetch()]);
			toast.success(t("settings.aiMemory.retryQueued"));
		} catch (error) {
			toast.error(error instanceof Error ? error.message : t("settings.aiMemory.retryError"));
		} finally {
			setAiMemoryRetrying(null);
		}
	};
	const handleDecisionProviderSettingsSave = async () => {
		setDecisionProviderSaving(true);
		try {
			await saveDecisionProviderSettings(decisionProviderDraft);
			await decisionProviderSettingsQuery.refetch();
			toast.success(t("settings.decisionProvider.saved"));
		} catch (error) {
			toast.error(error instanceof Error ? error.message : t("settings.decisionProvider.saveError"));
		} finally {
			setDecisionProviderSaving(false);
		}
	};

	const sections = useMemo<SettingsSectionMeta[]>(
		() => [
			{
				id: "general",
				group: "workspace",
				keywords: t("settings.navigation.keywords.general"),
				label: t("settings.sections.general.label"),
				description: t("settings.sections.general.description"),
				icon: Wrench,
			},
			{
				id: "appearance",
				group: "workspace",
				keywords: t("settings.navigation.keywords.appearance"),
				label: t("settings.sections.appearance.label"),
				description: t("settings.sections.appearance.description"),
				icon: SunMedium,
			},
			{
				id: "model",
				group: "services",
				keywords: t("settings.navigation.keywords.model"),
				label: t("settings.sections.model.label"),
				description: t("settings.sections.model.description"),
				icon: Sparkles,
			},
			{
				id: "decisionProvider",
				group: "services",
				keywords: t("settings.navigation.keywords.decisionProvider"),
				label: t("settings.sections.decisionProvider.label"),
				description: t("settings.sections.decisionProvider.description"),
				icon: BrainCircuit,
			},
			{
				id: "aiMemory",
				group: "services",
				keywords: t("settings.navigation.keywords.aiMemory"),
				label: t("settings.sections.aiMemory.label"),
				description: t("settings.sections.aiMemory.description"),
				icon: Database,
			},
			{
				id: "integrations",
				group: "services",
				keywords: t("settings.navigation.keywords.integrations"),
				label: t("settings.sections.integrations.label"),
				description: t("settings.sections.integrations.description"),
				icon: Cable,
			},
			{
				id: "computerUse",
				group: "services",
				keywords: t("settings.navigation.keywords.computerUse"),
				label: t("settings.sections.computerUse.label"),
				description: t("settings.sections.computerUse.description"),
				icon: MonitorCog,
			},
			{
				id: "connections",
				group: "services",
				keywords: t("settings.navigation.keywords.connections"),
				label: t("settings.sections.connections.label"),
				description: t("settings.sections.connections.description"),
				icon: Server,
			},
			{
				id: "shortcuts",
				group: "workspace",
				keywords: t("settings.navigation.keywords.shortcuts"),
				label: t("settings.sections.shortcuts.label"),
				description: t("settings.sections.shortcuts.description"),
				icon: Keyboard,
			},
			{
				id: "git",
				group: "advanced",
				keywords: t("settings.navigation.keywords.git"),
				label: t("settings.sections.git.label"),
				description: t("settings.sections.git.description"),
				icon: GitBranch,
			},
			{
				id: "experimental",
				group: "advanced",
				keywords: t("settings.navigation.keywords.experimental"),
				label: t("settings.sections.experimental.label"),
				description: t("settings.sections.experimental.description"),
				icon: Package,
			},
			{
				id: "account",
				group: "services",
				keywords: t("settings.navigation.keywords.account"),
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
	const uxMetrics = useMemo(() => readUxMetrics(), [open, uxMetricsVersion]);
	const uxMetricNames: DccUxMetricName[] = [
		"first_prompt",
		"diff_discovered",
		"terminal_discovered",
		"terminal_scope_switched",
		"advanced_composer_control_used",
		"command_palette_action",
	];

	return (
		<>
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="dcc-settings-dialog w-[min(94vw,1140px)] sm:max-w-[1140px] p-0 gap-0"
				onOpenAutoFocus={() => {
					returnFocusRef.current = document.activeElement instanceof HTMLElement
						? document.activeElement
						: null;
				}}
				onCloseAutoFocus={(event) => {
					if (returnFocusRef.current?.isConnected) {
						event.preventDefault();
						returnFocusRef.current.focus();
					}
				}}
				onEscapeKeyDown={(event) => {
					// Radix handles Escape in capture; let a nonempty search clear first.
					if (
						event.target instanceof HTMLInputElement &&
						event.target.hasAttribute("data-settings-search") &&
						event.target.value
					) event.preventDefault();
				}}
			>
				<div className="dcc-settings-layout">
					<SettingsNavigation
						sections={sections}
						activeSection={activeSection}
						onSelect={setActiveSection}
						panelId={panelId}
						workspaceName={workspaceName}
						workspaceRoot={workspaceRoot}
					/>
					<div className="dcc-settings-main">
						<header className="dcc-settings-header">
							<p className="dcc-settings-eyebrow">{t(`settings.navigation.groups.${activeMeta.group}`)}</p>
							<DialogTitle className="sr-only">{t("settings.title")}</DialogTitle>
							<h2 id={headingId}>{activeMeta.label}</h2>
							<DialogDescription>{activeMeta.description}</DialogDescription>
						</header>
						<div id={panelId} role="region" aria-labelledby={headingId} ref={contentRef} className="dcc-settings-content" tabIndex={0}>

							{activeSection === "general" ? (
								<section className="space-y-4">
									<div className="rounded-xl border border-border/60 bg-muted/15 p-4 space-y-3">
										<h3 className="text-sm font-medium">{t("frontendRecovery.settingsTitle")}</h3>
										<p className="text-xs text-muted-foreground">{t("frontendRecovery.settingsBody")}</p>
										<FrontendDiagnostics />
									</div>
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
								<section className="dcc-settings-appearance">
									<div className="dcc-settings-preference">
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
									<SettingsThemePicker theme={theme} onChange={onThemeChange} />
									<div className="dcc-settings-preference dcc-settings-preference-row">
										<div>
											<h3 className="text-[14px] font-medium text-foreground">
												{t("settings.appearance.density")}
											</h3>
											<p className="mt-1 text-[12px] text-muted-foreground">
												{t("settings.appearance.densityHint")}
											</p>
										</div>
										<ToggleGroup
											type="single"
											value={density}
											onValueChange={(value) => {
												if (value === "comfortable" || value === "compact") {
													onDensityChange(value);
												}
											}}
											className="gap-1"
										>
											<ToggleGroupItem
												value="comfortable"
												className="h-8 rounded-lg border border-border/60 px-3 text-[12px]"
											>
												{t("settings.appearance.comfortable")}
											</ToggleGroupItem>
											<ToggleGroupItem
												value="compact"
												className="h-8 rounded-lg border border-border/60 px-3 text-[12px]"
											>
												{t("settings.appearance.compact")}
											</ToggleGroupItem>
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
									<ProviderAvailabilityPanel
										providers={providers}
										pendingProviderIds={pendingAvailabilityProviderIds}
										errors={availabilityErrors}
										onChange={(providerId, enabled) => {
											void handleProviderAvailabilityChange(providerId, enabled);
										}}
									/>
									<ProviderAccountUsagePanel
										providers={providers}
										runtimeSettings={providerRuntimeSettings}
									/>
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

							{activeSection === "decisionProvider" ? (
								<section className="space-y-4">
									<Tabs
										value={decisionProviderTab}
										onValueChange={setDecisionProviderTab}
										className="space-y-4"
									>
										<TabsList className="w-full">
											<TabsTrigger value="configuration" className="flex-1">
												{t("settings.decisionProvider.tabs.configuration", {
													defaultValue: "Configuração",
												})}
											</TabsTrigger>
											<TabsTrigger value="history" className="flex-1">
												{t("settings.decisionProvider.tabs.history", {
													defaultValue: "Histórico de decisões",
												})}
											</TabsTrigger>
										</TabsList>
										<TabsContent value="configuration" className="mt-0">
									<div className="rounded-xl border border-border/60 bg-muted/15 p-4">
										<div className="flex items-start gap-3">
											<div className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary">
												<BrainCircuit className="size-4" />
											</div>
											<div className="min-w-0">
												<h3 className="text-[14px] font-medium text-foreground">
													{t("settings.decisionProvider.title")}
												</h3>
												<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
													{t("settings.decisionProvider.hint")}
												</p>
											</div>
										</div>
										<div className="mt-4 grid gap-4 sm:grid-cols-2">
											<div className="space-y-2">
												<span className="text-[12px] font-medium text-foreground">
													{t("settings.decisionProvider.providerLabel")}
												</span>
												<div className="flex h-8 items-center gap-3">
													<Switch
														checked={decisionProviderDraft.provider === "typesafe"}
														onCheckedChange={(enabled) =>
															setDecisionProviderDraft((current) => ({
																...current,
																provider: enabled ? "typesafe" : "disabled",
															}))
														}
														aria-label={t("settings.decisionProvider.providerLabel")}
													/>
													<span className="text-[12px] text-muted-foreground">
														{decisionProviderDraft.provider === "typesafe"
															? t("settings.decisionProvider.active")
															: t("settings.decisionProvider.inactive")}
													</span>
												</div>
												<p className="text-[11px] text-muted-foreground">
													TypeSafe Jev
												</p>
											</div>
											<div className="space-y-2">
												<span className="text-[12px] font-medium text-foreground">
													{t("settings.decisionProvider.modeLabel")}
												</span>
												<ToggleGroup
													type="single"
													value={decisionProviderDraft.mode}
													onValueChange={(mode) => {
														if (mode) setDecisionProviderDraft((current) => ({ ...current, mode }));
													}}
													className="justify-start"
														>
															<ToggleGroupItem value="observe">{t("settings.decisionProvider.modes.observe")}</ToggleGroupItem>
															<ToggleGroupItem value="enforce">{t("settings.decisionProvider.modes.enforce")}</ToggleGroupItem>
														</ToggleGroup>
													</div>
													<div className="space-y-2">
														<span className="text-[12px] font-medium text-foreground">
																{t("settings.decisionProvider.modelRoutingModeLabel")}
														</span>
														<div className="flex h-8 items-center gap-3">
														<Switch
															checked={decisionProviderDraft.modelRouting === "automatic"}
															onCheckedChange={(automatic) =>
																setDecisionProviderDraft((current) => ({
																	...current,
																	modelRouting: automatic ? "automatic" : "manual",
																}))
															}
															aria-label={t("settings.decisionProvider.modelRoutingLabel")}
														/>
														<span className="text-[12px] text-muted-foreground">
															{decisionProviderDraft.modelRouting === "automatic"
																? t("settings.decisionProvider.modelRoutingModes.automatic")
																: t("settings.decisionProvider.modelRoutingModes.manual")}
														</span>
													</div>
															<p className="text-[11px] text-muted-foreground">
																{t("settings.decisionProvider.modelRoutingHint")}
															</p>
														</div>
			<div className="space-y-2">
				<span className="text-[12px] font-medium text-foreground">{t("settings.decisionProvider.apiKeyLabel")}</span>
				<Input
					type="password"
					value={decisionProviderDraft.apiKey ?? ""}
					onChange={(event) => setDecisionProviderDraft((current) => ({ ...current, apiKey: event.target.value || null, clearApiKey: false }))}
					placeholder={decisionProviderSettingsQuery.data?.apiKeyConfigured ? t("settings.decisionProvider.apiKeyConfigured") : t("settings.decisionProvider.apiKeyPlaceholder")}
					autoComplete="new-password"
				/>
				{decisionProviderSettingsQuery.data?.apiKeyConfigured ? (
					<Button
						type="button"
						variant="ghost"
						size="sm"
						className="h-7 px-2 text-[11px]"
						onClick={() => setDecisionProviderDraft((current) => ({ ...current, apiKey: null, clearApiKey: true }))}
					>
						{decisionProviderDraft.clearApiKey ? t("settings.decisionProvider.clearPending") : t("settings.decisionProvider.clearKey")}
					</Button>
				) : null}
			</div>
											<label className="space-y-2">
												<span className="text-[12px] font-medium text-foreground">{t("settings.decisionProvider.baseUrlLabel")}</span>
												<Input value={decisionProviderDraft.baseUrl ?? ""} onChange={(event) => setDecisionProviderDraft((current) => ({ ...current, baseUrl: event.target.value || null }))} />
											</label>
											<label className="space-y-2">
												<span className="text-[12px] font-medium text-foreground">{t("settings.decisionProvider.modelLabel")}</span>
												<Input value={decisionProviderDraft.model ?? ""} onChange={(event) => setDecisionProviderDraft((current) => ({ ...current, model: event.target.value || null }))} />
											</label>
										</div>
										<div className="mt-5 border-t border-border/50 pt-4">
											<h4 className="text-[13px] font-medium text-foreground">
												{t("settings.decisionProvider.pointsTitle")}
											</h4>
											<p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">
												{t("settings.decisionProvider.pointsHint")}
											</p>
											<div className="mt-3 grid gap-4 sm:grid-cols-2">
												{([
													{
														key: "memory_filter",
														enabled: decisionProviderDraft.memoryFilterEnabled,
														threshold: decisionProviderDraft.memoryThreshold,
														thresholdKey: "memoryThreshold",
														enabledKey: "memoryFilterEnabled",
														label: t("settings.decisionProvider.points.memory_filter.label"),
														hint: t("settings.decisionProvider.points.memory_filter.hint"),
														thresholdLabel: t("settings.decisionProvider.points.memory_filter.threshold"),
													},
													{
														key: "skill_router",
														enabled: decisionProviderDraft.skillRouterEnabled,
														threshold: decisionProviderDraft.skillConfidenceThreshold,
														thresholdKey: "skillConfidenceThreshold",
														enabledKey: "skillRouterEnabled",
														label: t("settings.decisionProvider.points.skill_router.label"),
														hint: t("settings.decisionProvider.points.skill_router.hint"),
														thresholdLabel: t("settings.decisionProvider.points.skill_router.threshold"),
													},
													{
														key: "model_router",
														enabled: decisionProviderDraft.modelRouterEnabled,
														threshold: decisionProviderDraft.modelConfidenceThreshold,
														thresholdKey: "modelConfidenceThreshold",
														enabledKey: "modelRouterEnabled",
														label: t("settings.decisionProvider.points.model_router.label"),
														hint: t("settings.decisionProvider.points.model_router.hint"),
														thresholdLabel: t("settings.decisionProvider.points.model_router.threshold"),
													},
													{
														key: "completion_review",
														enabled: decisionProviderDraft.completionReviewEnabled,
														threshold: decisionProviderDraft.completionThreshold,
														thresholdKey: "completionThreshold",
														enabledKey: "completionReviewEnabled",
														label: t("settings.decisionProvider.points.completion_review.label"),
														hint: t("settings.decisionProvider.points.completion_review.hint"),
														thresholdLabel: t("settings.decisionProvider.points.completion_review.threshold"),
													},
												] as const).map((point) => (
														<div key={point.key} className="rounded-lg border border-border/50 p-3">
															<div className="flex items-center justify-between gap-3">
																<span className="text-[12px] font-medium text-foreground">{point.label}</span>
																<div className="flex items-center gap-2">
																<Switch
																checked={point.enabled}
																onCheckedChange={(enabled) =>
																	setDecisionProviderDraft((current) => ({
																		...current,
																		[point.enabledKey]: enabled,
																	}))
															}
																	aria-label={point.label}
																/>
																	<span className="text-[11px] text-muted-foreground">
																		{point.enabled
																			? t("settings.decisionProvider.active")
																		: t("settings.decisionProvider.inactive")}
																	</span>
																</div>
															</div>
														<p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">{point.hint}</p>
														<label className="mt-2 block space-y-1">
															<span className="text-[11px] text-muted-foreground">{point.thresholdLabel}</span>
															<Input
																type="number"
																min={0}
																max={1}
																step={0.05}
																value={point.threshold ?? 0.65}
																onChange={(event) =>
																	setDecisionProviderDraft((current) => ({
																		...current,
																		[point.thresholdKey]: Number(event.target.value),
																	}))
																}
															/>
														</label>
													</div>
												))}
											</div>
										</div>
										<p className="mt-4 text-[11px] leading-relaxed text-muted-foreground">
											{t("settings.decisionProvider.securityHint")}
										</p>
										<div className="mt-4 flex flex-wrap items-center gap-2">
											<Button type="button" size="sm" disabled={decisionProviderSaving || decisionProviderSettingsQuery.isPending} onClick={() => void handleDecisionProviderSettingsSave()}>
												{decisionProviderSaving ? <Loader2 className="size-3.5 animate-spin" /> : null}
												{t("settings.decisionProvider.save")}
											</Button>
											{decisionProviderSettingsQuery.data?.apiKeyConfigured ? <Badge variant="secondary">{t("settings.decisionProvider.configured")}</Badge> : null}
											</div>
									</div>
										</TabsContent>
										<TabsContent value="history" className="mt-0">

									<div className="flex min-h-0 flex-col rounded-xl border border-border/60 p-4">
										<div className="flex items-start justify-between gap-4">
											<div>
												<h3 className="text-[14px] font-medium text-foreground">{t("settings.decisionProvider.historyTitle")}</h3>
												<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">{t("settings.decisionProvider.historyHint")}</p>
											</div>
											<Button
												type="button"
												variant="outline"
												size="sm"
												disabled={decisionProviderHistoryQuery.isFetching}
												onClick={() => void decisionProviderHistoryQuery.refetch()}
											>
												{decisionProviderHistoryQuery.isFetching ? <Loader2 className="size-3.5 animate-spin" /> : <RefreshCw className="size-3.5" />}
												{t("settings.decisionProvider.refreshHistory")}
											</Button>
										</div>
										{decisionProviderHistoryQuery.isPending ? (
											<p className="mt-4 text-[12px] text-muted-foreground">{t("settings.decisionProvider.historyLoading")}</p>
										) : decisionProviderHistoryQuery.data?.length ? (
													<ScrollArea className="mt-4 h-[min(60dvh,480px)] min-h-0 rounded-lg border border-border/50 bg-background">
												<div className="divide-y divide-border/40">
													{decisionProviderHistoryQuery.data.map((entry) => (
														<div className="space-y-1.5 p-3" key={entry.id}>
															<div className="flex flex-wrap items-center justify-between gap-2">
																<div className="flex flex-wrap items-center gap-2">
																	<Badge variant={entry.status === "completed" ? "secondary" : "destructive"} className="h-6 px-2 text-[10px] font-normal">
																		{t(`settings.decisionProvider.historyStatus.${entry.status}`)}
																	</Badge>
																	<Badge variant="outline" className="h-6 px-2 text-[10px] font-normal">
																		{t(`settings.decisionProvider.historyModes.${entry.mode}`)}
																	</Badge>
																	<Badge variant="outline" className="h-6 px-2 text-[10px] font-normal">
																		{t(`settings.decisionProvider.historyPoints.${entry.decisionPoint}`)}
																	</Badge>
																	{entry.status === "completed" && entry.decisionPoint === "completion_review" ? (
																		<Badge
																			variant={entry.selectedLabels.some((label) => label.startsWith("needs_review:")) ? "destructive" : "secondary"}
																			className="h-6 px-2 text-[10px] font-normal"
																		>
																{entry.selectedLabels.includes("kept")
																	? t("settings.decisionProvider.completionKeptLabel")
																	: entry.selectedLabels.some((label) => label.startsWith("needs_review:"))
																		? t("settings.decisionProvider.completionNeedsReview")
																		: t("settings.decisionProvider.completionComplete")}
																		</Badge>
																	) : null}
																	<span className="font-mono text-[11px] text-foreground">{entry.model}</span>
																</div>
																<span className="text-[11px] text-muted-foreground">{formatAiMemoryTimestamp(entry.createdAt, i18n.resolvedLanguage ?? i18n.language)}</span>
															</div>
															<p className="text-[11px] text-muted-foreground">
																{entry.status === "failed"
																	? t("settings.decisionProvider.historyFailedMeta", { candidates: entry.candidateCount, duration: entry.durationMs })
																	: t("settings.decisionProvider.historyMeta", {
																		candidates: entry.candidateCount,
																		selected: entry.selectedCount,
																		threshold: entry.threshold,
																		duration: entry.durationMs,
																	})}
															</p>
																	{entry.status === "completed" ? (
																		<p className="font-mono text-[11px] text-muted-foreground">
																			{entry.selectedLabels.length
																			? t("settings.decisionProvider.selectedLabels", { labels: entry.selectedLabels.join(", ") })
																			: t("settings.decisionProvider.selectedIndices", {
																				indices: entry.selectedIndices.length ? entry.selectedIndices.join(", ") : t("settings.decisionProvider.none"),
																			})}
																		</p>
															) : null}
															{entry.error ? <p className="text-[11px] text-destructive">{entry.error}</p> : null}
														</div>
													))}
												</div>
											</ScrollArea>
										) : (
											<p className="mt-4 text-[12px] text-muted-foreground">{t("settings.decisionProvider.historyEmpty")}</p>
											)}
										</div>
										</TabsContent>
									</Tabs>
								</section>
							) : null}

							{activeSection === "aiMemory" ? (
								<section className="space-y-4">
									<Tabs
										value={aiMemoryTab}
										onValueChange={setAiMemoryTab}
										className="space-y-4"
									>
										<TabsList className="w-full">
											<TabsTrigger value="configuration" className="flex-1">
												{t("settings.aiMemory.tabs.configuration", {
													defaultValue: "Configuração",
												})}
											</TabsTrigger>
											<TabsTrigger value="history" className="flex-1">
												{t("settings.aiMemory.tabs.history", {
													defaultValue: "Histórico de exportações",
												})}
											</TabsTrigger>
										</TabsList>
										<TabsContent value="configuration" className="mt-0 space-y-4">
									<div className="rounded-xl border border-border/60 bg-muted/15 p-4">
										<div className="flex items-start justify-between gap-4">
											<div className="flex min-w-0 items-start gap-3">
												<div className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary">
													<Database className="size-4" />
												</div>
												<div className="min-w-0">
													<h3 className="text-[14px] font-medium text-foreground">
														{t("settings.aiMemory.title")}
													</h3>
														<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
															{t("settings.aiMemory.hint")}
														</p>
														<p className="mt-2 text-[11px] leading-relaxed text-muted-foreground/90">
															{t("settings.aiMemory.operationsHint")}
														</p>
												</div>
											</div>
											<Button
												type="button"
												variant="outline"
												size="sm"
												disabled={aiMemoryStatusQuery.isFetching}
												onClick={() => void aiMemoryStatusQuery.refetch()}
											>
												{aiMemoryStatusQuery.isFetching ? (
													<Loader2 className="size-3.5 animate-spin" />
												) : (
													<RefreshCw className="size-3.5" />
												)}
												{t("settings.aiMemory.refresh")}
											</Button>
										</div>
										<div className="mt-4 rounded-lg border border-border/50 bg-background p-3">
											<div className="flex items-center justify-between gap-3">
												<div className="flex items-center gap-2 text-[12px] font-medium text-foreground">
													{aiMemoryStatusQuery.data?.mode === "managed" || aiMemoryStatusQuery.data?.mode === "remote" ? (
														<CircleCheck className="size-4 text-emerald-500" />
													) : (
														<CircleAlert className="size-4 text-amber-500" />
													)}
													{t("settings.aiMemory.status")}
												</div>
												<Badge variant="outline" className="h-7 px-2.5 text-[11px] font-normal">
													{t(`settings.aiMemory.modes.${aiMemoryStatusQuery.data?.mode ?? "unknown"}`)}
												</Badge>
											</div>
											<p className="mt-2 text-[12px] leading-relaxed text-muted-foreground">
												{aiMemoryStatusQuery.isPending
													? t("settings.aiMemory.loading")
													: aiMemoryStatusQuery.isError
														? t("settings.aiMemory.error")
															: t(`settings.aiMemory.statusBody.${aiMemoryStatusQuery.data?.mode ?? "unknown"}`)}
													</p>
													{aiMemoryStatusQuery.data?.message ? (
														<p className="mt-2 text-[11px] leading-relaxed text-destructive">
															{aiMemoryStatusQuery.data.message}
														</p>
													) : null}
																		</div>
																		</div>
																		<div className="rounded-xl border border-border/60 p-4">
																		<div className="grid gap-4 sm:grid-cols-2">
										<div className="sm:col-span-2">
											<p className="text-[11px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
												{t("settings.aiMemory.modeLabel")}
											</p>
											<ToggleGroup
												type="single"
												value={aiMemoryDraft.mode}
												onValueChange={(value) => {
													if (value === "managed" || value === "remote" || value === "disabled") {
														setAiMemoryDraft((current) => ({ ...current, mode: value }));
													}
												}}
												className="mt-2 flex-wrap justify-start gap-1"
											>
												{(["managed", "remote", "disabled"] as const).map((mode) => (
													<ToggleGroupItem
														key={mode}
														value={mode}
														className="h-8 rounded-lg border border-border/60 px-3 text-[12px]"
													>
														{t(`settings.aiMemory.modes.${mode}`)}
													</ToggleGroupItem>
												))}
											</ToggleGroup>
										</div>
										<label className="space-y-1.5">
											<span className="text-[12px] font-medium text-foreground">{t("settings.aiMemory.url")}</span>
											<Input
												value={aiMemoryDraft.baseUrl ?? ""}
												disabled={aiMemoryDraft.mode !== "remote"}
												placeholder="http://127.0.0.1:49374"
												onChange={(event) => setAiMemoryDraft((current) => ({ ...current, baseUrl: event.target.value || null }))}
											/>
										</label>
										<label className="space-y-1.5">
											<span className="text-[12px] font-medium text-foreground">{t("settings.aiMemory.tokenLabel")}</span>
											<Input
												type="password"
												value={aiMemoryDraft.token ?? ""}
												placeholder={aiMemorySettingsQuery.data?.tokenConfigured ? t("settings.aiMemory.tokenConfigured") : t("settings.aiMemory.tokenPlaceholder")}
												onChange={(event) => setAiMemoryDraft((current) => ({ ...current, token: event.target.value }))}
											/>
										</label>
										<p className="sm:col-span-2 text-[11px] leading-relaxed text-muted-foreground">{t("settings.aiMemory.connectionHint")}</p>
										<details className="sm:col-span-2 rounded-lg border border-border/50 bg-muted/10 px-3 py-2">
											<summary className="cursor-pointer text-[12px] font-medium text-foreground">{t("settings.aiMemory.advancedTitle")}</summary>
													<p className="mt-2 text-[11px] leading-relaxed text-muted-foreground">{t("settings.aiMemory.advancedHint")}</p>
													<p className="mt-2 text-[11px] leading-relaxed text-muted-foreground/90">{t("settings.aiMemory.technicalHint")}</p>
											<div className="mt-3 grid gap-4 sm:grid-cols-2">
												<label className="space-y-1.5">
													<span className="text-[12px] font-medium text-foreground">{t("settings.aiMemory.workspaceLabel")}</span>
													<Input value={aiMemoryDraft.workspace} onChange={(event) => setAiMemoryDraft((current) => ({ ...current, workspace: event.target.value }))} />
												</label>
												<label className="space-y-1.5">
													<span className="text-[12px] font-medium text-foreground">{t("settings.aiMemory.projectLabel")}</span>
													<Input value={aiMemoryDraft.project} onChange={(event) => setAiMemoryDraft((current) => ({ ...current, project: event.target.value }))} />
												</label>
												<label className="space-y-1.5 sm:col-span-2">
													<span className="text-[12px] font-medium text-foreground">{t("settings.aiMemory.dataDir")}</span>
													<Input value={aiMemoryDraft.dataDir ?? ""} onChange={(event) => setAiMemoryDraft((current) => ({ ...current, dataDir: event.target.value || null }))} placeholder={t("settings.aiMemory.dataDirPlaceholder")} />
												</label>
											</div>
										</details>
									</div>
									<div className="mt-4 flex flex-wrap items-center justify-between gap-2 border-t border-border/40 pt-3">
										<p className="text-[11px] leading-relaxed text-muted-foreground">{t("settings.aiMemory.settingsHint")}</p>
										<Button type="button" size="sm" disabled={aiMemorySaving || aiMemorySettingsQuery.isPending} onClick={() => void handleAiMemorySettingsSave()}>
											{aiMemorySaving ? <Loader2 className="size-3.5 animate-spin" /> : null}
											{t("settings.aiMemory.save")}
										</Button>
										{aiMemoryRestartRequired ? (
											<Button type="button" variant="outline" size="sm" onClick={() => void restartDcc()}>
												{t("settings.aiMemory.restartNow")}
											</Button>
										) : null}
																																				</div>
																																				</div>
																																				{aiMemoryStatusQuery.data ? (
																																					<div className="grid gap-3 sm:grid-cols-2">
																																						{(
																																							[
																																							["settings.aiMemory.dataDir", aiMemoryStatusQuery.data.dataDir],
																																							["settings.aiMemory.logPath", aiMemoryStatusQuery.data.logPath],
																																							["settings.aiMemory.version", aiMemoryStatusQuery.data.version],
																																							["settings.aiMemory.url", aiMemoryStatusQuery.data.url],
																																						] as Array<[string, string | null]>
																																					).map(([label, value]) => (
																																					<div className="rounded-xl border border-border/60 p-3" key={label}>
																																						<p className="text-[11px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
																																							{t(label)}
																																						</p>
																																						<div className="mt-2 flex items-center justify-between gap-2">
																																							<p className="min-w-0 truncate font-mono text-[11px] text-foreground" title={value ?? undefined}>
																																							{value ?? t("settings.aiMemory.notAvailable")}
																																						</p>
																																						{value ? (
																																							<Button
																																								type="button"
																																								variant="ghost"
																																								size="icon"
																																								className="size-7 shrink-0"
																																								aria-label={t("settings.aiMemory.copy")}
																																								onClick={() => {
																																								void navigator.clipboard?.writeText(value);
																																								toast.success(t("settings.aiMemory.copied"));
																																							}}
																																							>
																																								<Copy className="size-3.5" />
																																							</Button>
																																						) : null}
																																				</div>
																																				</div>
																																				))}
																																				</div>
																																				) : null}
																																				</TabsContent>

															<TabsContent value="history" className="mt-0 space-y-4">
															<div className="rounded-xl border border-border/60 p-4">
													<div className="flex items-start justify-between gap-4">
														<div>
															<h3 className="text-[14px] font-medium text-foreground">{t("settings.aiMemory.outboxTitle")}</h3>
															<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">{t("settings.aiMemory.outboxHint")}</p>
														</div>
															<Button type="button" variant="outline" size="sm" disabled={aiMemoryOutboxQuery.isFetching || aiMemoryExportHistoryQuery.isFetching} onClick={() => void Promise.all([aiMemoryOutboxQuery.refetch(), aiMemoryExportHistoryQuery.refetch()])}>
																{aiMemoryOutboxQuery.isFetching || aiMemoryExportHistoryQuery.isFetching ? <Loader2 className="size-3.5 animate-spin" /> : <RefreshCw className="size-3.5" />}
															{t("settings.aiMemory.refresh")}
														</Button>
													</div>
													{aiMemoryOutboxQuery.data?.length ? (
														<div className="mt-4 divide-y divide-border/40 rounded-lg border border-border/50 bg-background">
															{aiMemoryOutboxQuery.data.map((entry) => (
																<div className="flex flex-wrap items-center justify-between gap-3 p-3" key={entry.sessionId}>
																	<div className="min-w-0">
																		<p className="truncate font-mono text-[11px] text-foreground" title={entry.sessionId}>{entry.sessionId}</p>
																		<p className="mt-1 text-[11px] text-muted-foreground">
									{t("settings.aiMemory.outboxMeta", { attempts: entry.attempts, next: entry.nextAttemptAt, events: entry.eventCount })}
																		</p>
																		{entry.lastError ? <p className="mt-1 text-[11px] text-destructive">{entry.lastError}</p> : null}
																	</div>
																	<Button type="button" variant="outline" size="sm" disabled={aiMemoryRetrying === entry.sessionId} onClick={() => void handleAiMemoryRetry(entry.sessionId)}>
																	{aiMemoryRetrying === entry.sessionId ? <Loader2 className="size-3.5 animate-spin" /> : null}
																	{t("settings.aiMemory.retry")}
																</Button>
															</div>
															))}
														</div>
													) : (
														<p className="mt-4 rounded-lg border border-border/50 bg-background p-3 text-[12px] text-muted-foreground">{t("settings.aiMemory.outboxEmpty")}</p>
													)}
														</div>

														<div className="mt-4 border-t border-border/40 pt-4">
															<h4 className="text-[12px] font-medium text-foreground">{t("settings.aiMemory.historyTitle")}</h4>
															<p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">{t("settings.aiMemory.historyHint")}</p>
													{aiMemoryExportHistoryQuery.data?.length ? (
									<ScrollArea className="mt-3 max-h-72 rounded-lg border border-border/50 bg-background">
										<div className="divide-y divide-border/40">
										{aiMemoryExportHistoryQuery.data.map((entry) => (
																<div className="p-3" key={entry.id}>
																	<div className="flex flex-wrap items-center justify-between gap-2">
																		<p className="truncate font-mono text-[11px] text-foreground" title={entry.sessionId}>{t("settings.aiMemory.historySession", { id: entry.sessionId.length > 12 ? `${entry.sessionId.slice(0, 8)}…${entry.sessionId.slice(-4)}` : entry.sessionId })}</p>
																		<Badge variant={entry.status === "completed" ? "secondary" : "outline"} className="h-6 px-2 text-[10px] font-normal">{t(`settings.aiMemory.historyStatus.${entry.status}`)}</Badge>
																	</div>
															<p className="mt-1 text-[11px] text-muted-foreground" title={entry.finishedAt}>{t("settings.aiMemory.historyMeta", { attempts: entry.attempts, accepted: entry.acceptedCount, events: entry.eventCount, finished: formatAiMemoryTimestamp(entry.finishedAt, i18n.resolvedLanguage ?? i18n.language) })}</p>
																	{entry.error ? <p className="mt-1 text-[11px] text-destructive">{entry.error}</p> : null}
											</div>
										))}
										</div>
									</ScrollArea>
															) : (
																<p className="mt-3 text-[11px] text-muted-foreground">{t("settings.aiMemory.historyEmpty")}</p>
															)}
														</div>

										</TabsContent>
									</Tabs>
								</section>
							) : null}

							{activeSection === "integrations" ? (
								<McpIntegrationsPanel
									projectId={projectId}
									sessionId={sessionId}
									sessionProviderId={sessionProviderId}
									sessionCreatedAt={sessionCreatedAt}
									workspaceName={workspaceName}
									providerCatalog={providerCatalog}
								/>
							) : null}

							{activeSection === "computerUse" ? (
								<ComputerUsePanel
									sessionId={sessionId}
								/>
							) : null}

							{activeSection === "connections" ? (
								<PairedDevicesPanel defaultBackendUrl="http://127.0.0.1:9876" />
							) : null}

							{activeSection === "shortcuts" ? (
								<section className="space-y-4">
									<AppshotsShortcut />
									<div className="rounded-xl border border-border/60 p-4">
										<div className="flex items-start justify-between gap-4">
											<div>
												<h3 className="text-[14px] font-medium text-foreground">{t("settings.shortcuts.keyboardShortcuts")}</h3>
												<p className="mt-1 text-[12px] text-muted-foreground">
													{t("settings.shortcuts.keyboardShortcutsHint")}
												</p>
											</div>
											<div className="flex flex-col items-end gap-2">
												<SectionHeaderBadge>
													{t("settings.shortcuts.activeCount", { count: 11 })}
												</SectionHeaderBadge>
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
									<div className="rounded-xl border border-border/60 bg-muted/15 p-4">
										<div className="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
											<div className="flex min-w-0 items-start gap-3">
												<div className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary">
													<ListChecks className="size-4" />
												</div>
												<div className="min-w-0">
													<h3 className="text-[14px] font-medium text-foreground">{t("settings.git.automationTitle")}</h3>
													<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">{t("settings.git.automationHint")}</p>
													<p className="mt-2 truncate font-mono text-[11px] text-muted-foreground" title={workspaceRoot ?? undefined}>
														{workspaceName ?? workspaceRoot ?? t("settings.git.noWorkspace")}
													</p>
												</div>
											</div>
											<Button type="button" size="sm" disabled={!workspaceRoot?.trim()} onClick={() => setAutomationOpen(true)}>
												{t("automation.open")}
											</Button>
										</div>
										<div className="mt-4 border-t border-border/50 pt-3 text-[11px] leading-relaxed text-muted-foreground">
											<p>{t("settings.git.sharingHint")}</p>
											<p className="mt-1">{t("automation.noPolling")}</p>
										</div>
									</div>
								</section>
							) : null}

							{activeSection === "experimental" ? (
								<section className="space-y-4">
									<div className="rounded-xl border border-border/60 bg-muted/15 p-4">
										<div className="flex items-start justify-between gap-4">
											<div>
												<h3 className="text-[14px] font-medium text-foreground">
													{t("settings.experimental.uxMetricsTitle")}
												</h3>
												<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
													{t("settings.experimental.uxMetricsHint")}
												</p>
											</div>
											<Button
												type="button"
												variant="outline"
												size="sm"
												onClick={() => {
													window.localStorage.removeItem(DCC_UX_METRICS_STORAGE_KEY);
													setUxMetricsVersion((value) => value + 1);
												}}
											>
												{t("settings.experimental.resetMetrics")}
											</Button>
										</div>
										<div className="mt-4 divide-y divide-border/40 rounded-lg bg-background px-3">
											{uxMetricNames.map((name) => {
												const metric = uxMetrics[name];
												return (
													<div key={name} className="flex items-center justify-between gap-4 py-2.5 text-[12px]">
														<span className="text-foreground">
															{t(`settings.experimental.metrics.${name}`)}
														</span>
														<span className="tabular-nums text-muted-foreground">
															{metric
																? t("settings.experimental.metricValue", {
																	count: metric.count,
																	seconds: (metric.firstElapsedMs / 1000).toFixed(1),
																})
																: t("settings.experimental.metricEmpty")}
														</span>
													</div>
												);
											})}
										</div>
									</div>
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
									<CodeRabbitCliIntegrationCard />
								</section>
							) : null}
						</div>
					</div>
				</div>
			</DialogContent>
		</Dialog>
		<WorkspaceProjectAutomationDialog
			open={automationOpen}
			onOpenChange={setAutomationOpen}
			workspaceRoot={workspaceRoot}
			onWorkspaceChanged={() => {
				const root = workspaceRoot?.trim();
				if (!root) return;
				void queryClient.invalidateQueries({ queryKey: [WORKSPACE_GIT_STATUS_QUERY_KEY, root] });
				void queryClient.invalidateQueries({ queryKey: [WORKSPACE_GIT_BRANCH_DIFF_QUERY_KEY, root] });
			}}
		/>
		</>
	);
}
