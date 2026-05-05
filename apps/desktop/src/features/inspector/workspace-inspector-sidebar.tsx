import { useQuery, useQueryClient } from "@tanstack/react-query";
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
	workspaceGhPrViewWeb,
	workspaceGhPrCreateFill,
	workspaceGhPrMerge,
	workspaceGitCommitPush,
	workspaceGitStageAll,
	workspaceGitPush,
} from "@/lib/workspace-api";
import { useWorkspaceGitStatus, WORKSPACE_GIT_STATUS_QUERY_KEY } from "./use-workspace-git-status";
import { useWorkspacePrStatus, WORKSPACE_PR_STATUS_QUERY_KEY } from "./use-workspace-pr-status";
import { EmptyState } from "@/features/panel";
import type { CoreEvent, ProviderCatalog } from "@dcc/contracts";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { getProviderChips, summarizeProviderHealth } from "@/features/providers/provider-display";
import { getGithubCliStatus, openGithubCliAuthTerminal } from "@/lib/github-cli";
import { sessionStateLabel } from "@/i18n/session-state-label";

type WorkspaceInspectorSidebarProps = {
	providerCatalog: ProviderCatalog | null;
	sessionSnapshot: RuntimeSessionSnapshot | null;
	workspaceId: string | null;
	workspaceName: string | null;
	workspaceBranch: string | null;
	workspacePath: string | null;
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

function inspectorActionTitle(mode: string) {
	switch (mode) {
		case "create-pr":
			return "Criar PR";
		case "open-pr":
			return "Abrir PR";
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
	workspaceId,
	workspaceName,
	workspaceBranch,
	workspacePath,
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
	const prStatusQuery = useWorkspacePrStatus(
		workspacePath,
		gitBranch,
	);
	const githubCliStatusQuery = useQuery({
		queryKey: ["githubCliStatus"],
		queryFn: getGithubCliStatus,
		staleTime: 60_000,
		refetchOnWindowFocus: true,
	});
	const [isContinuingWorkspace, setIsContinuingWorkspace] = useState(false);
	const rootRef = useRef<HTMLDivElement | null>(null);
	const hasWorkingTreeChanges =
		(gitStatusQuery.data?.staged.length ?? 0) > 0 ||
		(gitStatusQuery.data?.unstaged.length ?? 0) > 0;
	const githubCliStatus = githubCliStatusQuery.data ?? null;
	const githubCliReady = githubCliStatus?.status === "ready";
	const githubCliMessage =
		githubCliStatus?.message ??
		(githubCliStatusQuery.isPending ? "Checking GitHub CLI..." : null);
	const prStatus = prStatusQuery.data ?? null;
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

		const loadingToast = toast.loading(`${inspectorActionTitle(commitMode)}...`);

		if (commitMode === "create-pr" && !githubCliReady) {
			toast.dismiss(loadingToast);
			const reason = githubCliMessage ?? "GitHub CLI não encontrado ou não autenticado.";
			toast.warning(reason, {
				description: 'Execute "gh auth login" no Terminal e tente novamente.',
				action: {
					label: "Abrir Terminal",
					onClick: () => openGithubCliAuthTerminal(),
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
					await workspaceGhPrViewWeb({ workspaceRoot: root });
					toast.info("This PR is closed. Open it in the browser if you need to inspect it.", {
						id: loadingToast,
					});
					return;
				case "push":
					await workspaceGitPush({ workspaceRoot: root });
					toast.success("Pushed", { id: loadingToast });
					break;
				case "open-pr":
					await workspaceGhPrViewWeb({ workspaceRoot: root });
					toast.success("Opened PR in browser", { id: loadingToast });
					break;
				case "merge":
					await workspaceGhPrMerge({ workspaceRoot: root });
					toast.success("PR merged", { id: loadingToast });
					break;
				case "fix":
				case "resolve-conflicts":
					await workspaceGhPrViewWeb({ workspaceRoot: root });
					toast.info("Open the PR to inspect checks and conflicts.", {
						id: loadingToast,
					});
					break;
				case "create-pr": {
					if (hasWorkingTreeChanges) {
						throw new Error("Commit local changes before creating a PR.");
					}
					await workspaceGhPrCreateFill({ workspaceRoot: root });
					toast.success("PR created", { id: loadingToast });
					break;
				}
				case "commit-and-push":
				default: {
					await workspaceGitStageAll({ workspaceRoot: root, relativePath: "." });
					const message = `chore: checkpoint for ${workspaceName ?? "workspace"}`;
					await workspaceGitCommitPush({ workspaceRoot: root, message });
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
		} catch (error) {
			const message = getInspectorActionErrorMessage(error);
			console.error("[inspector] git action failed", { commitMode, root, error });
			toast.error(`${inspectorActionTitle(commitMode)} failed: ${message}`, {
				id: loadingToast,
			});
			throw error;
		}
	}, [commitMode, githubCliMessage, githubCliReady, queryClient, workspacePath, workspaceName]);

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
				baseBranch: workspaceBranch,
			});
			if (!result?.success) {
				throw new Error("Unable to continue workspace.");
			}
			toast.success("Workspace continued", { id: loadingToast });
			await queryClient.invalidateQueries({
				queryKey: [WORKSPACE_GIT_STATUS_QUERY_KEY, root],
			});
			await queryClient.invalidateQueries({
				queryKey: [WORKSPACE_PR_STATUS_QUERY_KEY, root],
			});
		} catch (error) {
			const message = getInspectorActionErrorMessage(error);
			toast.error(`Continue failed: ${message}`, { id: loadingToast });
			throw error;
		} finally {
			setIsContinuingWorkspace(false);
		}
	}, [currentBranch, queryClient, workspaceBranch, workspacePath]);

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
					prUrl={prStatus?.url ?? null}
					prNumber={prStatus?.number ?? null}
					prProvider={prStatus?.provider ?? null}
				/>

				<div className="flex min-h-0 flex-1 flex-col gap-2 overflow-hidden px-3 pb-3 pt-2">
					<div className="shrink-0">
						<BranchToolbar branch={workspaceBranch ?? ""} workspacePath={workspacePath} />
					</div>
					{pathLine ? (
						<p
							className="shrink-0 truncate text-[11px] text-muted-foreground"
							title={workspacePath ?? undefined}
						>
							{pathLine}
						</p>
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
	);
}
