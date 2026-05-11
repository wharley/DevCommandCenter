import { useQueryClient } from "@tanstack/react-query";
import {
	useCallback,
	useEffect,
	useMemo,
	useRef,
	useState,
	type MouseEvent as ReactMouseEvent,
	type ReactNode,
} from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { BranchToolbar } from "@/components/BranchToolbar";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import type { WorkspaceGitPreviewSelection } from "./workspace-git-file-preview";
import { SessionEventFeed } from "@/features/sessions/session-event-feed";
import type { RuntimeSessionSnapshot } from "@/features/sessions/session-workbench";
import { InspectorChangesSection } from "./inspector-changes-section";
import { GitSectionHeader } from "./git-section-header";
import { projectWorkspaceMessages } from "@/features/panel/thread-projection";
import { PlanReviewCard } from "@/features/panel/message-components";
import { derivePlanFollowUpState } from "@/features/panel/plan-follow-up";
import { resolveCommitMode } from "@/features/commit/WorkspaceCommitButton.logic";
import {
	workspaceContinueFromBaseBranch,
	workspaceChangeRequestViewWeb,
	workspaceChangeRequestCreate,
	workspaceChangeRequestMerge,
	workspaceGitCommitPush,
	workspaceGitStageAll,
	workspaceGitPush,
	workspaceRunSetup,
} from "@/lib/workspace-api";
import { useWorkspaceGitStatus, WORKSPACE_GIT_STATUS_QUERY_KEY } from "./use-workspace-git-status";
import { WORKSPACE_GIT_BRANCH_DIFF_QUERY_KEY } from "./use-workspace-git-branch-diff";
import { useWorkspacePrStatus, WORKSPACE_PR_STATUS_QUERY_KEY } from "./use-workspace-pr-status";
import {
	useWorkspaceForgeContext,
	WORKSPACE_FORGE_CONTEXT_QUERY_KEY,
} from "./use-workspace-forge-context";
import { EmptyState } from "@/features/panel";
import type { CoreEvent, ProviderCatalog, Repository, WorkspaceSetupReport } from "@dcc/contracts";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { getProviderChips, summarizeProviderHealth } from "@/features/providers/provider-display";
import { ForgeConnectDialog } from "@/features/settings/forge-connect-dialog";
import {
	invalidateForgeCliQueries,
	useForgeCliAccounts,
} from "@/features/settings/forge-cli-queries";
import { useForgeCliLoginsHealth } from "@/features/settings/use-forge-cli-logins-health";
import { getDefaultForgeHost, setForgeCliSelectedLogin } from "@/lib/forge-cli";
import { sessionStateLabel } from "@/i18n/session-state-label";
import type { WorkspaceStatus } from "@/features/workspaces/types";
import { setupReportDescription } from "@/features/workspaces/workspace-setup-report";
import type { ForgeCliProvider } from "@dcc/contracts";

type WorkspaceInspectorSidebarProps = {
	providerCatalog: ProviderCatalog | null;
	sessionSnapshot: RuntimeSessionSnapshot | null;
	currentRepository: Repository | null;
	workspaceId: string | null;
	workspaceName: string | null;
	workspaceBranch: string | null;
	workspacePath: string | null;
	workspaceStatus: WorkspaceStatus | null;
	workspaceSetupReport: WorkspaceSetupReport | null;
	selectedProviderLabel: string | null;
	selectedModelLabel: string | null;
	sessionState: string;
	sessionId: string | null;
	sessionEvents: CoreEvent[];
	selectedPreview: WorkspaceGitPreviewSelection | null;
	onSelectPreview: (selection: WorkspaceGitPreviewSelection | null) => void;
	activeTab: InspectorTab;
	onTabChange: (tab: InspectorTab) => void;
};

const MIN_SECTION_HEIGHT = 128;
const MAX_SECTION_HEIGHT = 640;
const INITIAL_CHANGES_HEIGHT = 200;

type InspectorTab = "activity" | "context" | "plan";

function DetailRow({ label, children }: { label: string; children: ReactNode }) {
	return (
		<div className="flex gap-3 border-b border-border/35 py-2 text-[11px] leading-snug last:border-b-0">
			<span className="w-[76px] shrink-0 font-medium uppercase tracking-[0.06em] text-muted-foreground">
				{label}
			</span>
			<div className="min-w-0 flex-1 font-mono text-[11.5px] text-foreground">{children}</div>
		</div>
	);
}

function changeRequestLabel(provider?: string | null): "PR" | "MR" {
	return provider === "gitlab" ? "MR" : "PR";
}

function forgeProviderLabel(provider: ForgeCliProvider): "GitHub" | "GitLab" {
	return provider === "gitlab" ? "GitLab" : "GitHub";
}

function resolveForgeContext(
	provider?: string | null,
	host?: string | null,
): {
	provider: ForgeCliProvider;
	host: string;
	providerLabel: "GitHub" | "GitLab";
	requestLabel: "PR" | "MR";
} {
	const normalizedProvider: ForgeCliProvider = provider === "gitlab" ? "gitlab" : "github";
	const normalizedHost = host?.trim() || getDefaultForgeHost(normalizedProvider);
	return {
		provider: normalizedProvider,
		host: normalizedHost,
		providerLabel: forgeProviderLabel(normalizedProvider),
		requestLabel: changeRequestLabel(normalizedProvider),
	};
}

function forgeIdentityInitials(value: string): string {
	return value
		.split(/[\s@._/-]+/)
		.map((part) => part.trim())
		.filter(Boolean)
		.slice(0, 2)
		.map((part) => part[0]?.toUpperCase() ?? "")
		.join("") || "FG";
}

function forgeProviderDotClass(provider: ForgeCliProvider): string {
	return provider === "gitlab" ? "bg-[#FC6D26]" : "bg-foreground";
}

function inspectorActionTitle(mode: string, requestLabel: "PR" | "MR") {
	switch (mode) {
		case "create-pr":
			return `Criar ${requestLabel}`;
		case "open-pr":
			return `Abrir ${requestLabel}`;
		case "commit-and-push":
			return "Commitar e enviar";
		case "push":
			return "Enviar";
		case "fix":
			return "Corrigir CI";
		case "resolve-conflicts":
			return "Resolver conflitos";
		case "merge":
			return "Mesclar";
		case "merged":
			return "Mesclado";
		case "closed":
			return "Fechado";
		default:
			return "Ação";
	}
}

function getInspectorActionErrorMessage(error: unknown): string {
	if (typeof error === "string" && error.trim().length > 0) {
		return error.trim();
	}
	if (error && typeof error === "object") {
		const candidate = error as Record<string, unknown>;
		if (typeof candidate.message === "string" && candidate.message.trim().length > 0) {
			return candidate.message.trim();
		}
		if (typeof candidate.error === "string" && candidate.error.trim().length > 0) {
			return candidate.error.trim();
		}
		if (typeof candidate.toString === "function") {
			const text = candidate.toString();
			if (typeof text === "string" && text !== "[object Object]" && text.trim().length > 0) {
				return text.trim();
			}
		}
	}
	return "Action failed";
}

function ProviderCatalogDense({ catalog }: { catalog: ProviderCatalog | null }) {
	const providers = catalog?.providers ?? [];

	if (providers.length === 0) {
		return (
			<p className="rounded-md border border-dashed border-border/60 px-3 py-2 text-[11px] text-muted-foreground">
				No providers registered in the catalog yet.
			</p>
		);
	}

	return (
		<div className="divide-y divide-border/45 rounded-md border border-border/50 bg-muted/10">
			{providers.map((provider) => (
				<div key={provider.id} className="px-2.5 py-2">
					<div className="flex flex-wrap items-start justify-between gap-2">
						<span className="text-[11.5px] font-medium leading-tight">{provider.label}</span>
						<span className="max-w-[min(100%,220px)] text-right text-[10px] text-muted-foreground">
							{provider.description}
						</span>
					</div>
					<div className="mt-1.5 flex flex-wrap gap-1">
						<Badge variant={provider.stable ? "success" : "outline"} className="text-[10px] font-normal">
							{provider.stable ? "stable" : "experimental"}
						</Badge>
						<Badge variant={summarizeProviderHealth(provider.health).variant} className="text-[10px] font-normal">
							{summarizeProviderHealth(provider.health).label}
						</Badge>
						{getProviderChips(provider)
							.filter(
								(chip) =>
									chip.label !== "stable" &&
									chip.label !== "experimental" &&
									chip.label !== summarizeProviderHealth(provider.health).label,
							)
							.map((chip) => (
								<Badge key={`${provider.id}-${chip.label}`} variant={chip.variant} className="text-[10px] font-normal">
									{chip.label}
								</Badge>
							))}
						{provider.models.map((model) => (
							<Badge
								key={`${provider.id}-${model.id}`}
								variant={model.recommended ? "success" : "outline"}
								className="text-[10px] font-normal"
							>
								{model.label}
							</Badge>
						))}
					</div>
				</div>
			))}
		</div>
	);
}

function ResizeHandle({
	label,
	onMouseDown,
}: {
	label: string;
	onMouseDown: (event: ReactMouseEvent<HTMLButtonElement>) => void;
}) {
	return (
		<button
			type="button"
			aria-label={label}
			onMouseDown={onMouseDown}
			className="group flex h-2 shrink-0 cursor-row-resize items-center justify-center bg-sidebar outline-none"
		>
			<span className="h-px w-full bg-border/70 transition-colors group-hover:bg-foreground/30 group-focus-visible:bg-foreground/40" />
		</button>
	);
}

/**
 * Right rail: Git (clone Helmor) + session activity / context integrated from App props — no placeholder cards.
 */
export function WorkspaceInspectorSidebar({
	providerCatalog,
	sessionSnapshot,
	currentRepository,
	workspaceId,
	workspaceName,
	workspaceBranch,
	workspacePath,
	workspaceStatus,
	workspaceSetupReport,
	selectedProviderLabel,
	selectedModelLabel,
	sessionState,
	sessionId,
	sessionEvents,
	selectedPreview,
	onSelectPreview,
	activeTab,
	onTabChange,
}: WorkspaceInspectorSidebarProps) {
	const { t } = useTranslation("common");
	const hasWorkspace = Boolean(workspaceId && workspaceName && workspaceBranch);
	const pathLine =
		workspacePath && workspacePath.length > 0
			? workspacePath.length > 56
				? `…${workspacePath.slice(-55)}`
				: workspacePath
			: null;
	const queryClient = useQueryClient();
	const gitStatusQuery = useWorkspaceGitStatus(workspacePath);
	const gitBranch =
		gitStatusQuery.data?.currentBranch &&
		gitStatusQuery.data.currentBranch !== "HEAD"
			? gitStatusQuery.data.currentBranch
			: null;
	const currentBranch = gitBranch ?? workspaceBranch ?? "";
	const repositoryId = currentRepository?.id?.trim() || null;
	const workspaceForgeContextQuery = useWorkspaceForgeContext(workspacePath);
	const workspaceForgeContext = workspaceForgeContextQuery.data ?? null;
	const forgeContext = resolveForgeContext(
		workspaceForgeContext?.provider,
		workspaceForgeContext?.host,
	);
	const forgeAccountsQuery = useForgeCliAccounts(forgeContext.provider, forgeContext.host, {
		enabled: Boolean(workspaceForgeContext?.provider && workspaceForgeContext?.host),
	});
	useForgeCliLoginsHealth(forgeContext.provider, forgeContext.host, {
		enabled: Boolean(workspaceForgeContext?.provider && workspaceForgeContext?.host),
	});
	const selectedForgeLogin = workspaceForgeContext?.effectiveLogin ?? null;
	const forgeAccounts = forgeAccountsQuery.data?.accounts ?? [];
	const boundForgeLogin = workspaceForgeContext?.boundLogin?.trim() || null;
	const boundForgeAccount = useMemo(() => {
		if (!boundForgeLogin) {
			return null;
		}
		return (
			forgeAccounts.find((account) => account.login === boundForgeLogin) ?? null
		);
	}, [boundForgeLogin, forgeAccounts]);
	const prStatusQuery = useWorkspacePrStatus(
		workspacePath,
		gitBranch,
		selectedForgeLogin,
	);
	const prStatus = prStatusQuery.data ?? null;
	const [forgeConnectOpen, setForgeConnectOpen] = useState(false);
	const [isContinuingWorkspace, setIsContinuingWorkspace] = useState(false);
	const [isRetryingSetup, setIsRetryingSetup] = useState(false);
	const rootRef = useRef<HTMLDivElement | null>(null);
	const hasWorkingTreeChanges =
		(gitStatusQuery.data?.staged.length ?? 0) > 0 ||
		(gitStatusQuery.data?.unstaged.length ?? 0) > 0;
	const forgeCliReady = workspaceForgeContext?.status === "ready";
	const forgeNeedsConnect = workspaceForgeContext?.remoteState === "unauthenticated";
	const forgeUnavailable = workspaceForgeContext?.remoteState === "unavailable";
	const forgeConnected = workspaceForgeContext?.remoteState === "ok";
	const forgeIdentityLabel =
		boundForgeAccount?.name?.trim() || boundForgeLogin || forgeContext.providerLabel;
	const forgeIdentitySubtitle = boundForgeLogin ? `@${boundForgeLogin}` : null;
	const isSetupPending = workspaceStatus === "setup_pending";
	const setupReportSummary =
		workspaceSetupReport == null
			? null
			: setupReportDescription(t, workspaceSetupReport, []);
	const forgeCliMessage =
		workspaceForgeContext?.message ??
		(workspaceForgeContextQuery.isPending
			? `Checking ${forgeContext.providerLabel} CLI...`
			: null);
	const commitMode = resolveCommitMode({
		branch: currentBranch,
		prStatus,
		gitStatus: gitStatusQuery.data ?? null,
	});

	const handleInspectorCommit = useCallback(async () => {
		const root = workspacePath?.trim();
		if (!root) {
			toast.error("No workspace path");
			throw new Error("No workspace path");
		}

		const loadingToast = toast.loading(
			`${inspectorActionTitle(commitMode, forgeContext.requestLabel)}...`,
		);

		if (commitMode === "create-pr" && forgeNeedsConnect) {
			toast.dismiss(loadingToast);
			const reason =
				forgeCliMessage ??
				`${forgeContext.providerLabel} CLI não encontrado ou não autenticado.`;
			toast.warning(reason, {
				description: `Conecte o ${forgeContext.providerLabel} no terminal embutido e tente novamente.`,
				action: {
					label: `Conectar ${forgeContext.providerLabel}`,
					onClick: () => setForgeConnectOpen(true),
				},
				duration: 12_000,
			});
			return;
		}

		try {
			switch (commitMode) {
				case "merged":
					return;
				case "closed":
					await workspaceChangeRequestViewWeb({
						workspaceRoot: root,
						forgeLogin: selectedForgeLogin,
					});
					toast.info(
						`Este ${forgeContext.requestLabel} está fechado. Abra no navegador se precisar inspecionar.`,
						{ id: loadingToast },
					);
					return;
				case "push":
					await workspaceGitPush({
						workspaceRoot: root,
						forgeLogin: selectedForgeLogin,
					});
					toast.success("Pushed", { id: loadingToast });
					break;
				case "open-pr":
					await workspaceChangeRequestViewWeb({
						workspaceRoot: root,
						forgeLogin: selectedForgeLogin,
					});
					toast.success(`${forgeContext.requestLabel} aberto no navegador`, {
						id: loadingToast,
					});
					break;
				case "merge":
					await workspaceChangeRequestMerge({
						workspaceRoot: root,
						forgeLogin: selectedForgeLogin,
					});
					toast.success(`${forgeContext.requestLabel} mesclado`, { id: loadingToast });
					break;
				case "fix":
				case "resolve-conflicts":
					await workspaceChangeRequestViewWeb({
						workspaceRoot: root,
						forgeLogin: selectedForgeLogin,
					});
					toast.info(`Abra o ${forgeContext.requestLabel} para inspecionar checks e conflitos.`, {
						id: loadingToast,
					});
					break;
				case "create-pr": {
					if (hasWorkingTreeChanges) {
						throw new Error(
							`Commit local changes before creating a ${forgeContext.requestLabel}.`,
						);
					}
					await workspaceChangeRequestCreate({
						workspaceRoot: root,
						forgeLogin: selectedForgeLogin,
					});
					toast.success(`${forgeContext.requestLabel} criado`, { id: loadingToast });
					break;
				}
				case "commit-and-push":
				default: {
					await workspaceGitStageAll({ workspaceRoot: root, relativePath: "." });
					const message = `chore: checkpoint for ${workspaceName ?? "workspace"}`;
					await workspaceGitCommitPush({
						workspaceRoot: root,
						message,
						forgeLogin: selectedForgeLogin,
					});
					toast.success("Committed and pushed", { id: loadingToast });
					break;
				}
			}

			await queryClient.invalidateQueries({
				queryKey: [WORKSPACE_GIT_STATUS_QUERY_KEY, root],
			});
			await queryClient.invalidateQueries({
				queryKey: [WORKSPACE_PR_STATUS_QUERY_KEY, root],
			});
			await queryClient.invalidateQueries({
				queryKey: [WORKSPACE_FORGE_CONTEXT_QUERY_KEY, root],
			});
			await queryClient.invalidateQueries({
				queryKey: [WORKSPACE_GIT_BRANCH_DIFF_QUERY_KEY, root],
			});
		} catch (error) {
			const message = getInspectorActionErrorMessage(error);
			console.error("[inspector] git action failed", { commitMode, root, error });
			toast.error(
				`${inspectorActionTitle(commitMode, forgeContext.requestLabel)} failed: ${message}`,
				{
				id: loadingToast,
				},
			);
			throw error;
		}
	}, [
		commitMode,
		forgeCliMessage,
		forgeCliReady,
		forgeContext.providerLabel,
		forgeContext.requestLabel,
		queryClient,
		selectedForgeLogin,
		workspacePath,
		workspaceName,
	]);

	const handleContinueWorkspace = useCallback(async () => {
		const root = workspacePath?.trim();
		if (!root) {
			toast.error("No workspace path");
			throw new Error("No workspace path");
		}

		setIsContinuingWorkspace(true);
		const loadingToast = toast.loading("Continuing workspace...");
		try {
			const result = await workspaceContinueFromBaseBranch({
				workspaceRoot: root,
				baseBranch: null,
				targetBranch: prStatus?.baseBranch ?? null,
				newBranchName: workspaceName ?? null,
			});
			if (!result?.success) {
				throw new Error("Unable to continue workspace.");
			}
			toast.success(`Workspace moved to ${result.branch}`, { id: loadingToast });
			await queryClient.invalidateQueries({
				queryKey: ["workspaces"],
			});
			await queryClient.invalidateQueries({
				queryKey: [WORKSPACE_GIT_STATUS_QUERY_KEY, root],
			});
			if (result.workspaceRoot && result.workspaceRoot !== root) {
				await queryClient.invalidateQueries({
					queryKey: [WORKSPACE_GIT_STATUS_QUERY_KEY, result.workspaceRoot],
				});
			}
			await queryClient.invalidateQueries({
				queryKey: [WORKSPACE_PR_STATUS_QUERY_KEY, root],
			});
			await queryClient.invalidateQueries({
				queryKey: [WORKSPACE_FORGE_CONTEXT_QUERY_KEY, root],
			});
			await queryClient.invalidateQueries({
				queryKey: [WORKSPACE_GIT_BRANCH_DIFF_QUERY_KEY, root],
			});
			if (result.workspaceRoot && result.workspaceRoot !== root) {
				await queryClient.invalidateQueries({
					queryKey: [WORKSPACE_PR_STATUS_QUERY_KEY, result.workspaceRoot],
				});
				await queryClient.invalidateQueries({
					queryKey: [WORKSPACE_FORGE_CONTEXT_QUERY_KEY, result.workspaceRoot],
				});
				await queryClient.invalidateQueries({
					queryKey: [WORKSPACE_GIT_BRANCH_DIFF_QUERY_KEY, result.workspaceRoot],
				});
			}
		} catch (error) {
			const message = getInspectorActionErrorMessage(error);
			toast.error(`Continue failed: ${message}`, { id: loadingToast });
			throw error;
		} finally {
			setIsContinuingWorkspace(false);
		}
	}, [prStatus?.baseBranch, queryClient, workspaceName, workspacePath]);

	const handleRetrySetup = useCallback(async () => {
		const root = workspacePath?.trim();
		if (!root) {
			toast.error("No workspace path");
			throw new Error("No workspace path");
		}

		setIsRetryingSetup(true);
		const loadingToast = toast.loading(t("inspector.setupRetry.loading"));
		try {
			const result = await workspaceRunSetup({ workspaceRoot: root });
			const description = setupReportDescription(t, result.setupReport, result.setupHints);

			switch (result.setupReport.status) {
				case "completed":
					toast.success(t("inspector.setupRetry.successTitle"), {
						id: loadingToast,
						description,
					});
					break;
				case "warning":
					toast.warning(t("inspector.setupRetry.pendingTitle"), {
						id: loadingToast,
						description,
					});
					break;
				case "failed":
					toast.error(t("inspector.setupRetry.pendingTitle"), {
						id: loadingToast,
						description,
					});
					break;
				default:
					toast.success(t("inspector.setupRetry.successTitle"), {
						id: loadingToast,
						description,
					});
					break;
			}

			await queryClient.invalidateQueries({
				queryKey: ["workspaces"],
			});
			await queryClient.invalidateQueries({
				queryKey: [WORKSPACE_GIT_STATUS_QUERY_KEY, root],
			});
			await queryClient.invalidateQueries({
				queryKey: [WORKSPACE_PR_STATUS_QUERY_KEY, root],
			});
			await queryClient.invalidateQueries({
				queryKey: [WORKSPACE_GIT_BRANCH_DIFF_QUERY_KEY, root],
			});
		} catch (error) {
			const message = getInspectorActionErrorMessage(error);
			toast.error(t("inspector.setupRetry.errorTitle"), {
				id: loadingToast,
				description: message,
			});
			throw error;
		} finally {
			setIsRetryingSetup(false);
		}
	}, [queryClient, t, workspacePath]);

	const handleSelectForgeLogin = useCallback(
		async (login: string) => {
			const root = workspacePath?.trim();
			if (!root || !workspaceForgeContext?.provider || !workspaceForgeContext.host) {
				return;
			}

			await setForgeCliSelectedLogin(
				workspaceForgeContext.provider,
				workspaceForgeContext.host,
				login,
			);
			await invalidateForgeCliQueries(
				queryClient,
				workspaceForgeContext.provider,
				workspaceForgeContext.host,
			);
			await queryClient.invalidateQueries({
				queryKey: [WORKSPACE_FORGE_CONTEXT_QUERY_KEY, root],
			});
			await queryClient.invalidateQueries({
				queryKey: [WORKSPACE_PR_STATUS_QUERY_KEY, root],
			});
		},
		[queryClient, workspaceForgeContext, workspacePath],
	);

	const [changesHeight, setChangesHeight] = useState(INITIAL_CHANGES_HEIGHT);
	const [manualResize, setManualResize] = useState(false);
	const autoOpenedPlanMessageIdRef = useRef<string | null>(null);
	const planMessages = useMemo(
		() => projectWorkspaceMessages([], sessionEvents, sessionId, null),
		[sessionEvents, sessionId],
	);
	const planFollowUpState = useMemo(
		() => derivePlanFollowUpState(planMessages),
		[planMessages],
	);
	const activePlanMessage = planFollowUpState.activePlanMessage;
	const latestPlanMessage = planFollowUpState.latestPlanMessage;

	useEffect(() => {
		autoOpenedPlanMessageIdRef.current = null;
		onTabChange("activity");
	}, [sessionId]);

	useEffect(() => {
		const planMessageId = activePlanMessage?.id ?? null;
		if (!planMessageId) {
			return;
		}
		if (activeTab === "plan") {
			autoOpenedPlanMessageIdRef.current = planMessageId;
			return;
		}
		if (autoOpenedPlanMessageIdRef.current === planMessageId) {
			return;
		}
		autoOpenedPlanMessageIdRef.current = planMessageId;
		onTabChange("plan");
	}, [activePlanMessage?.id, activeTab, onTabChange]);

	useEffect(() => {
		const root = rootRef.current;
		if (!root || manualResize) {
			return;
		}

		const syncHeight = () => {
			const availableHeight = root.clientHeight;
			if (availableHeight <= 0) {
				return;
			}
			const half = Math.round(availableHeight / 2);
			setChangesHeight(
				Math.min(MAX_SECTION_HEIGHT, Math.max(MIN_SECTION_HEIGHT, half)),
			);
		};

		syncHeight();
		const observer = new ResizeObserver(syncHeight);
		observer.observe(root);
		return () => observer.disconnect();
	}, [manualResize]);

	function handleResizeGitSectionStart(event: ReactMouseEvent<HTMLButtonElement>) {
		event.preventDefault();
		setManualResize(true);
		const startY = event.clientY;
		const startHeight = changesHeight;
		const onMove = (moveEvent: MouseEvent) => {
			const delta = moveEvent.clientY - startY;
			setChangesHeight(
				Math.min(MAX_SECTION_HEIGHT, Math.max(MIN_SECTION_HEIGHT, startHeight + delta)),
			);
		};
		const onUp = () => {
			window.removeEventListener("mousemove", onMove);
			window.removeEventListener("mouseup", onUp);
		};
		window.addEventListener("mousemove", onMove);
		window.addEventListener("mouseup", onUp);
	}

	if (!hasWorkspace) {
		return (
			<div ref={rootRef} className="dcc-inspector flex h-full min-h-0 flex-col overflow-hidden text-foreground">
				<div className="flex min-h-0 flex-1 items-center justify-center px-4 py-6">
					<EmptyState
						title={t("inspector.empty.title")}
						description={t("inspector.empty.description")}
					/>
				</div>
			</div>
		);
	}

	const activityCount = sessionEvents.length;
	const catalogCount = providerCatalog?.providers.length ?? 0;

	return (
		<>
			<div
				ref={rootRef}
				className="dcc-inspector flex h-full min-h-0 flex-col overflow-hidden text-foreground"
				data-dcc-inspector-root
			>
			<section
				className="flex shrink-0 flex-col overflow-hidden border-b border-border/60"
				style={{ height: `${changesHeight}px` }}
			>
				<GitSectionHeader
					commitMode={commitMode}
					isRefreshing={gitStatusQuery.isFetching && !gitStatusQuery.isPending}
					onCommit={handleInspectorCommit}
					onContinueWorkspace={handleContinueWorkspace}
					isContinuingWorkspace={isContinuingWorkspace}
					onRetrySetup={handleRetrySetup}
					isRetryingSetup={isRetryingSetup}
					showRetrySetup={isSetupPending}
					retrySetupLabel={t("inspector.setupRetry.button")}
					prUrl={prStatus?.url ?? null}
					prNumber={prStatus?.number ?? null}
					prProvider={prStatus?.provider ?? null}
				/>

				<div className="flex min-h-0 flex-1 flex-col gap-2 overflow-hidden px-3 pb-3 pt-2">
					<div className="shrink-0">
						<BranchToolbar branch={currentBranch} workspacePath={workspacePath} />
					</div>
					{pathLine ? (
						<p
							className="shrink-0 truncate text-[11px] text-muted-foreground"
							title={workspacePath ?? undefined}
						>
							{pathLine}
						</p>
					) : null}
					{isSetupPending && setupReportSummary ? (
						<div className="shrink-0 rounded-md border border-amber-500/30 bg-amber-500/8 px-2.5 py-2 text-[11px] leading-relaxed text-amber-950 dark:text-amber-100">
							<p className="font-medium">{t("inspector.setupRetry.pendingTitle")}</p>
							<p className="mt-1 text-amber-900/80 dark:text-amber-100/80">
								{setupReportSummary}
							</p>
						</div>
					) : null}
					<div className="flex min-h-0 min-w-0 flex-1 flex-col">
						<InspectorChangesSection
							workspaceRoot={workspacePath}
							selectedPreview={selectedPreview}
							onSelectPreview={onSelectPreview}
						/>
					</div>
				</div>
			</section>

			<ResizeHandle label="Resize Git section" onMouseDown={handleResizeGitSectionStart} />

			<section className="flex min-h-0 flex-1 flex-col overflow-hidden border-t border-border/40">
				<Tabs
					value={activeTab}
					onValueChange={(value) => {
						if (value === "activity" || value === "context" || value === "plan") {
							onTabChange(value);
						}
					}}
					className="flex min-h-0 flex-1 flex-col gap-0"
				>
					<div className="flex shrink-0 items-center justify-between gap-2 border-b border-border/40 px-3 py-2">
						<div className="min-w-0">
							<p className="text-[11px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
								{t("inspector.gitSection.kicker")}
							</p>
							<p className="truncate text-[13px] font-medium leading-tight text-foreground">
								{t("inspector.gitSection.title")}
							</p>
						</div>
						<Badge variant="outline" className="h-6 shrink-0 px-2 text-[10px] font-normal">
							{t("inspector.gitSection.eventsCount", { count: activityCount })}
						</Badge>
					</div>

					<div className="shrink-0 border-b border-border/40 bg-muted/15 px-2">
							<TabsList variant="line" className="h-9 w-full justify-start gap-0 border-0 bg-transparent p-0">
								<TabsTrigger value="activity" className="h-9 rounded-none px-3 text-[12px]">
									{t("inspector.tabs.activity")}
								</TabsTrigger>
								<TabsTrigger value="context" className="h-9 rounded-none px-3 text-[12px]">
									{t("inspector.tabs.context")}
									{catalogCount > 0 ? (
										<span className="ml-1.5 tabular-nums text-[10px] text-muted-foreground">
											({catalogCount})
									</span>
									) : null}
							</TabsTrigger>
								<TabsTrigger value="plan" className="h-9 rounded-none px-3 text-[12px]">
									{t("inspector.tabs.plan")}
									{latestPlanMessage ? (
										<span className="ml-1.5 tabular-nums text-[10px] text-muted-foreground">
											(1)
										</span>
									) : null}
								</TabsTrigger>
						</TabsList>
					</div>

					<TabsContent
						value="activity"
						className="mt-0 flex min-h-0 flex-1 flex-col overflow-hidden data-[state=inactive]:hidden"
					>
						<SessionEventFeed events={sessionEvents} compact />
					</TabsContent>

						<TabsContent
							value="context"
							className="mt-0 min-h-0 flex-1 overflow-y-auto overscroll-contain px-3 pb-3 pt-2 data-[state=inactive]:hidden"
						>
							<div className="space-y-4">
								<div>
									<p className="mb-2 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
										{t("inspector.groups.workspace")}
									</p>
									<div className="rounded-md border border-border/50 bg-muted/10 px-2">
										<DetailRow label={t("inspector.fields.name")}>{workspaceName ?? "—"}</DetailRow>
										<DetailRow label={t("inspector.fields.id")}>{workspaceId ?? "—"}</DetailRow>
										<DetailRow label={t("inspector.fields.branch")}>{workspaceBranch ?? "—"}</DetailRow>
										<DetailRow label={t("inspector.fields.path")}>
											<span title={workspacePath ?? undefined}>{workspacePath ?? "—"}</span>
										</DetailRow>
									</div>
								</div>

								<div>
									<p className="mb-2 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
										{t("inspector.groups.composer")}
									</p>
									<div className="rounded-md border border-border/50 bg-muted/10 px-2">
										<DetailRow label={t("inspector.fields.provider")}>{selectedProviderLabel ?? "—"}</DetailRow>
										<DetailRow label={t("inspector.fields.model")}>{selectedModelLabel ?? "—"}</DetailRow>
									</div>
								</div>

								<div>
									<p className="mb-2 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
										{t("inspector.groups.runtime")}
									</p>
									<div className="rounded-md border border-border/50 bg-muted/10 px-2">
										<DetailRow label={t("inspector.fields.state")}>
											<span className="inline-flex items-center gap-2">
												{sessionStateLabel(sessionState, t)}
												{sessionId ? (
												<Badge variant="outline" className="font-mono text-[10px] font-normal">
													{sessionId.length > 14 ? `${sessionId.slice(0, 12)}…` : sessionId}
												</Badge>
											) : null}
										</span>
										</DetailRow>
										{sessionSnapshot ? (
											<>
												<DetailRow label={t("inspector.fields.turns")}>{String(sessionSnapshot.turnCount)}</DetailRow>
												<DetailRow label={t("inspector.fields.checkpoints")}>{String(sessionSnapshot.checkpointCount)}</DetailRow>
												<DetailRow label={t("inspector.fields.lastTurn")}>
													{sessionSnapshot.lastTurnPrompt ?? "—"}
												</DetailRow>
												<DetailRow label={t("inspector.fields.providerId")}>{sessionSnapshot.providerId}</DetailRow>
											</>
										) : (
											<DetailRow label={t("inspector.fields.session")}>{t("inspector.sessionFallback")}</DetailRow>
										)}
									</div>
								</div>

								<div>
									<p className="mb-2 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
										{t("inspector.groups.forge")}
									</p>
									<div className="rounded-md border border-border/50 bg-muted/10 p-3">
										<div className="flex items-center gap-3">
											<span
												aria-hidden
												className="flex size-10 shrink-0 items-center justify-center rounded-full border border-border/60 bg-background text-[13px] font-semibold uppercase text-foreground"
											>
												{forgeIdentityInitials(forgeIdentityLabel)}
											</span>
											<div className="min-w-0 flex-1">
												<div className="flex items-center gap-2">
													<span className="truncate text-[13px] font-semibold text-foreground">
														{forgeConnected
															? forgeIdentityLabel
															: `${forgeContext.providerLabel} ${t("settings.account.notReadyBadge").toLowerCase()}`}
													</span>
													{workspaceForgeContext?.provider ? (
														<span className="inline-flex items-center gap-1 rounded-full border border-border/60 bg-background/80 px-2 py-0.5 text-[10px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
															<span
																aria-hidden
																className={`size-1.5 rounded-full ${forgeProviderDotClass(
																	workspaceForgeContext.provider,
																)}`}
															/>
															{forgeProviderLabel(workspaceForgeContext.provider)}
														</span>
													) : null}
												</div>
												<div className="mt-0.5 text-[12px] text-muted-foreground">
													{forgeConnected
														? forgeIdentitySubtitle
														: forgeCliMessage ?? "—"}
												</div>
											</div>
											{workspaceForgeContext?.provider && workspaceForgeContext.host ? (
												<Button
													type="button"
													variant={forgeConnected ? "outline" : "default"}
													size="sm"
													onClick={() => setForgeConnectOpen(true)}
													className={!forgeConnected ? "px-4" : undefined}
												>
													{forgeConnected
														? t("settings.account.switchAccount")
														: t("settings.account.connect")}
												</Button>
											) : null}
										</div>
										<div className="mt-3 border-t border-border/45" />
										<DetailRow label={t("inspector.fields.host")}>
											{workspaceForgeContext?.host ?? "—"}
										</DetailRow>
										<DetailRow label={t("inspector.fields.remote")}>
											{workspaceForgeContext
												? `${workspaceForgeContext.remoteName}/${workspaceForgeContext.namespace}/${workspaceForgeContext.repo}`
												: "—"}
										</DetailRow>
										<DetailRow label={t("inspector.fields.knownHosts")}>
											{(workspaceForgeContext?.knownHosts?.length ?? 0) > 0
												? workspaceForgeContext?.knownHosts?.join(", ")
												: "—"}
										</DetailRow>
										<DetailRow label={t("inspector.fields.account")}>
											{workspaceForgeContext?.provider && workspaceForgeContext.host ? (
												<div className="flex flex-wrap gap-2">
													{forgeAccountsQuery.isPending ? (
														<span className="text-muted-foreground">
															{t("settings.account.checking")}
														</span>
													) : forgeAccounts.length > 0 ? (
														forgeAccounts.map((account) => {
															const active =
																account.login === selectedForgeLogin ||
																(!selectedForgeLogin && account.selected);
															const label = account.name
																? `${account.name} · @${account.login}`
																: account.login;
															return (
																<Button
																	key={account.login}
																	type="button"
																	variant={active ? "default" : "outline"}
																	size="sm"
																	title={account.email ?? undefined}
																	onClick={() => {
																		void handleSelectForgeLogin(account.login);
																	}}
																>
																	{label}
																</Button>
															);
														})
													) : (
														<span className="text-muted-foreground">—</span>
													)}
												</div>
											) : (
												"—"
											)}
										</DetailRow>
										<DetailRow label={t("inspector.fields.forgeStatus")}>
											<div className="flex flex-wrap items-center gap-2">
												<Badge
													variant={!forgeNeedsConnect && !forgeUnavailable ? "success" : "outline"}
													className="text-[10px] font-normal"
												>
													{!forgeNeedsConnect && !forgeUnavailable
														? t("settings.account.readyBadge")
														: t("settings.account.notReadyBadge")}
												</Badge>
												<span className="text-muted-foreground">
													{forgeConnected ? t("settings.account.accountLabel") : forgeCliMessage ?? "—"}
												</span>
											</div>
										</DetailRow>
									</div>
								</div>

								<div>
									<div className="mb-2 flex items-center justify-between gap-2">
										<p className="text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
											{t("inspector.groups.providers")}
										</p>
										{catalogCount > 0 ? (
											<Badge variant="secondary" className="h-5 text-[10px] font-normal">
												{t("inspector.providersRegistered", { count: catalogCount })}
											</Badge>
										) : null}
									</div>
									<ProviderCatalogDense catalog={providerCatalog} />
								</div>

								<p className="text-[10px] leading-relaxed text-muted-foreground">
									{t("inspector.terminalNote")}
								</p>
							</div>
						</TabsContent>

						<TabsContent
							value="plan"
							className="mt-0 min-h-0 flex-1 overflow-y-auto overscroll-contain px-3 pb-3 pt-2 data-[state=inactive]:hidden"
						>
							{latestPlanMessage ? (
								<PlanReviewCard
									plan={
										latestPlanMessage.plan ?? {
											title: "Plan",
											summary: latestPlanMessage.content,
											steps: [],
											approvedPrompts: [],
											rawMarkdown: latestPlanMessage.content,
											markdown: latestPlanMessage.content,
											isPlanLike: false,
											canCollapse: latestPlanMessage.content.length > 900,
											source: "plain",
										}
									}
									workspacePath={workspacePath}
								/>
							) : (
								<div className="flex min-h-full items-center justify-center px-4 py-8 text-center">
									<div className="max-w-sm">
										<p className="text-[13px] font-medium text-foreground">
											{t("inspector.plan.emptyTitle")}
										</p>
										<p className="mt-1 text-[11px] leading-5 text-muted-foreground">
											{t("inspector.plan.emptyDescription")}
										</p>
									</div>
								</div>
							)}
						</TabsContent>
				</Tabs>
			</section>
			</div>
			<ForgeConnectDialog
				open={forgeConnectOpen}
				onOpenChange={setForgeConnectOpen}
				provider={forgeContext.provider}
				host={forgeContext.host}
				repositoryId={repositoryId}
				onConnected={() => {
					void queryClient.invalidateQueries({
						queryKey: ["repositories"],
					});
					void queryClient.invalidateQueries({
						queryKey: [WORKSPACE_FORGE_CONTEXT_QUERY_KEY, workspacePath?.trim() ?? ""],
					});
					if (workspacePath?.trim()) {
						void queryClient.invalidateQueries({
							queryKey: [WORKSPACE_PR_STATUS_QUERY_KEY, workspacePath.trim()],
						});
					}
				}}
			/>
		</>
	);
}
