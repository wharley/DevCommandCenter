import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
	ArrowUpRight,
	Command,
	GitBranch,
	Folder,
	LoaderCircle,
	RefreshCw,
	X,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast, Toaster } from "sonner";
import type {
	ProviderCatalog,
	Repository,
	WorkspaceIsolationMode,
} from "@dcc/contracts";
import { Button } from "@/components/ui/button";
import { WorkspaceComposer } from "@/features/composer/WorkspaceComposer";
import type { ComposerSubmittedTurn } from "@/features/composer/composer-turn";
import { listProviders } from "@/lib/provider-api";
import {
	createWorkspaceForRepo,
	listRepositories,
	listWorkspaces,
} from "@/lib/workspace-api";
import { sendTurn, startThread } from "@/lib/session-api";
import {
	getProviderUnhealthyReason,
	isProviderEnabled,
	SELECTED_MODEL_STORAGE_KEY,
	SELECTED_PROVIDER_STORAGE_KEY,
} from "@/features/providers/provider-selection.logic";
import {
	draftToProviderRuntimeConfig,
	getProviderRuntimeDraft,
	readProviderRuntimeSettings,
} from "@/features/providers/provider-runtime-settings";
import { resolveDelegateTaskToolInstructions } from "@/features/sessions/delegate-task-tool-instructions";
import {
	beginQuickLaunch,
	checkpointQuickLaunch,
	hideQuickComposer,
	newQuickLaunch,
	openQuickTask,
	quickStatus,
	type QuickLaunch,
} from "./api";
import { executeQuickLaunch } from "./launch";

const SELECTION_KEY = "dcc.quickComposer.selection";
type Selection = {
	projectId: string;
	providerId: string;
	modelId: string;
	modes: Record<string, WorkspaceIsolationMode>;
};
function readSelection(): Selection {
	try {
		const saved = JSON.parse(localStorage.getItem(SELECTION_KEY) ?? "null");
		return {
			projectId: typeof saved?.projectId === "string" ? saved.projectId : "",
			providerId:
				typeof saved?.providerId === "string"
					? saved.providerId
					: (localStorage.getItem(SELECTED_PROVIDER_STORAGE_KEY) ?? ""),
			modelId:
				typeof saved?.modelId === "string"
					? saved.modelId
					: (localStorage.getItem(SELECTED_MODEL_STORAGE_KEY) ?? ""),
			modes: saved?.modes && typeof saved.modes === "object" ? saved.modes : {},
		};
	} catch {
		return { projectId: "", providerId: "", modelId: "", modes: {} };
	}
}

export function QuickComposer() {
	const { t, i18n } = useTranslation("common");
	const [selection, setSelection] = useState(readSelection);
	const [repositories, setRepositories] = useState<Repository[]>([]);
	const [providers, setProviders] = useState<ProviderCatalog["providers"]>([]);
	const [runtimeSettings, setRuntimeSettings] = useState(
		readProviderRuntimeSettings,
	);
	const [loading, setLoading] = useState(true);
	const [error, setError] = useState<string | null>(null);
	const [launch, setLaunch] = useState<QuickLaunch | null>(null);
	const [visibleCompletedLaunchId, setVisibleCompletedLaunchId] = useState<
		string | null
	>(null);
	const [reviewedLaunchId, setReviewedLaunchId] = useState<string | null>(null);
	const [busy, setBusy] = useState(false);
	const busyRef = useRef(false);
	const [focusKey, setFocusKey] = useState(0);
	const loadSequence = useRef(0);
	const repository = repositories.find(
		(repo) => repo.projectId === selection.projectId,
	);
	const provider = providers.find((item) => item.id === selection.providerId);
	const model = provider?.models.find((item) => item.id === selection.modelId);
	const mode: WorkspaceIsolationMode =
		selection.modes[selection.projectId] === "localDirect"
			? "localDirect"
			: "protectedWorktree";
	const providerRuntime = draftToProviderRuntimeConfig(
		provider ? getProviderRuntimeDraft(runtimeSettings, provider.id) : null,
		provider?.capabilities ?? null,
	);
	const unfinished =
		launch && launch.phase !== "completed" && reviewedLaunchId !== launch.id;
	const blocked =
		!repository ||
		!provider ||
		!model ||
		!isProviderEnabled(provider) ||
		!!getProviderUnhealthyReason(provider);

	const refresh = useCallback(async () => {
		const sequence = ++loadSequence.current;
		setLoading(true);
		try {
			const [repos, catalog, status, tasks] = await Promise.all([
				listRepositories(),
				listProviders(),
				quickStatus(),
				// A history lookup failure must not prevent composing a new task.
				listWorkspaces().catch(() => null),
			]);
			if (sequence !== loadSequence.current) return;
			setRepositories(repos.repositories);
			setProviders(catalog.catalog.providers);
			setRuntimeSettings(readProviderRuntimeSettings());
			if (!busyRef.current) {
				const previousTask = tasks?.workspaces.find(
					(task) => task.id === status.launch?.workspaceId,
				);
				setVisibleCompletedLaunchId(
					previousTask &&
						previousTask.state !== "completed" &&
						previousTask.state !== "archived"
						? (status.launch?.id ?? null)
						: null,
				);
				setLaunch((current) =>
					current?.id === status.launch?.id &&
					current &&
					status.launch &&
					current.revision > status.launch.revision
						? current
						: status.launch,
				);
			}
			setSelection((previous) => {
				const projectId =
					previous.projectId || repos.repositories[0]?.projectId || "";
				// Keep an unavailable saved choice visible; never silently send
				// to another project/provider when the saved one disappears.
				const selectedProvider =
					catalog.catalog.providers.find((p) => p.id === previous.providerId) ??
					(!previous.providerId
						? catalog.catalog.providers.find(
								(p) => isProviderEnabled(p) && !getProviderUnhealthyReason(p),
							)
						: undefined);
				return {
					...previous,
					projectId,
					providerId: previous.providerId || selectedProvider?.id || "",
					modelId:
						previous.modelId ||
						selectedProvider?.models.find((m) => m.recommended)?.id ||
						selectedProvider?.models[0]?.id ||
						"",
				};
			});
			setError(null);
			setFocusKey((key) => key + 1);
		} catch (cause) {
			if (sequence === loadSequence.current) setError(String(cause));
		} finally {
			if (sequence === loadSequence.current) setLoading(false);
		}
	}, []);
	useEffect(() => {
		void refresh();
		let disposed = false;
		const subscription = listen("quick-composer-shown", () => {
			const locale = localStorage.getItem("dcc.ui.locale");
			if (locale === "pt-BR" || locale === "en")
				void i18n.changeLanguage(locale);
			void refresh();
		});
		subscription
			.then((stop) => {
				if (disposed) stop();
			})
			.catch(() => {});
		return () => {
			disposed = true;
			loadSequence.current++;
			void subscription.then((stop) => stop()).catch(() => {});
		};
	}, [refresh, i18n]);
	useEffect(() => {
		try {
			localStorage.setItem(SELECTION_KEY, JSON.stringify(selection));
		} catch {
			setError(t("quickComposer.saveFailed"));
		}
	}, [selection, t]);
	useEffect(() => {
		const onEscape = (event: KeyboardEvent) => {
			// Nested menus/dialogs own Escape first.
			if (
				event.key === "Escape" &&
				!event.defaultPrevented &&
				!event.isComposing &&
				!document.querySelector(
					'[role="dialog"], [role="menu"], [data-state="open"][data-radix-popper-content-wrapper]',
				)
			) {
				event.preventDefault();
				void hideQuickComposer().catch((cause) => setError(String(cause)));
			}
		};
		window.addEventListener("keydown", onEscape);
		return () => window.removeEventListener("keydown", onEscape);
	}, []);

	const submit = async (turn: ComposerSubmittedTurn): Promise<boolean> => {
		if (
			busyRef.current ||
			blocked ||
			!repository ||
			!provider ||
			!model ||
			unfinished
		)
			return false;
		busyRef.current = true;
		setBusy(true);
		setError(null);
		try {
			const initial = newQuickLaunch({
				projectId: repository.projectId,
				rootPath: repository.rootPath,
				baseBranch: repository.baseBranch,
				isolationMode: mode,
				providerId: provider.id,
				modelId: model.id,
				providerRuntime,
				turn,
			});
			const result = await executeQuickLaunch(initial, {
				begin: beginQuickLaunch,
				checkpoint: checkpointQuickLaunch,
				onProgress: setLaunch,
				createWorkspace: async ({ request }) => {
					const output = await createWorkspaceForRepo({
						projectId: request.projectId,
						workspaceRoot: request.rootPath,
						baseBranch: request.baseBranch,
						isolationMode: request.isolationMode,
						name: turn.rawPrompt.trim().split("\n")[0].slice(0, 80),
					});
					return output.workspace.id;
				},
				startSession: async ({ workspaceId, request }) => {
					const output = await startThread({
						workspaceId: workspaceId!,
						projectId: request.projectId,
						providerId: request.providerId,
						model: request.modelId,
						providerRuntime: request.providerRuntime,
						title: turn.rawPrompt.trim().split("\n")[0].slice(0, 80),
					});
					return output.session.id;
				},
				send: async ({ sessionId, request }) => {
					await sendTurn({
						sessionId: sessionId!,
						prompt: request.turn.rawPrompt,
						...request.turn.envelope,
						providerId: request.providerId,
						model: request.modelId,
						providerRuntime: request.providerRuntime,
						toolInstructions: resolveDelegateTaskToolInstructions({
							provider,
							providers,
						}),
					});
				},
			});
			if (result.phase !== "completed") return false;
			setVisibleCompletedLaunchId(result.id);
			// Window dismissal is presentation only; a hide failure must not
			// restore a prompt that was already accepted by the agent.
			void hideQuickComposer().catch((cause) => setError(String(cause)));
			return true;
		} catch (cause) {
			setError(String(cause));
			try {
				setLaunch((await quickStatus()).launch);
			} catch {
				/* retain draft */
			}
			return false;
		} finally {
			busyRef.current = false;
			setBusy(false);
		}
	};
	const recover = async () => {
		if (!launch || busyRef.current) return;
		try {
			if (launch.phase === "failed") {
				setReviewedLaunchId(launch.id);
				return;
			}
			const next = await checkpointQuickLaunch({
				...launch,
				revision: launch.revision + 1,
				phase: "failed",
				error: t("quickComposer.interrupted"),
			});
			setLaunch(next);
			setReviewedLaunchId(next.id);
		} catch (cause) {
			setError(String(cause));
		}
	};
	return (
		<main className="flex h-dvh flex-col bg-background text-foreground">
			<div className="flex h-10 shrink-0 items-center gap-2 border-b border-border/60 px-5">
				<Command className="size-4 text-muted-foreground" />
				<h1 className="flex-1 text-sm font-medium">
					{t("quickComposer.title")}
				</h1>
				<Button
					size="icon"
					variant="ghost"
					aria-label={t("quickComposer.refresh")}
					disabled={busy}
					onClick={() => void refresh()}
				>
					<RefreshCw className="size-3.5" />
				</Button>
				<Button
					size="icon"
					variant="ghost"
					aria-label={t("quickComposer.close")}
					onClick={() =>
						void hideQuickComposer().catch((cause) => setError(String(cause)))
					}
				>
					<X className="size-4" />
				</Button>
			</div>
			<div className="flex min-h-0 flex-1 flex-col gap-3 overflow-auto px-5 py-4">
				<div className="flex items-center gap-3">
					<Folder className="size-4 shrink-0 text-muted-foreground" />
					<label className="sr-only" htmlFor="quick-project">
						{t("quickComposer.project")}
					</label>
					<select
						id="quick-project"
						className="h-9 min-w-0 flex-1 rounded-lg border border-border bg-background px-2 text-sm"
						value={selection.projectId}
						disabled={busy || loading || !!unfinished}
						onChange={(event) =>
							setSelection((s) => ({ ...s, projectId: event.target.value }))
						}
					>
						{!repository && (
							<option value={selection.projectId}>
								{t("quickComposer.selectProject")}
							</option>
						)}
						{repositories.map((repo) => (
							<option key={repo.id} value={repo.projectId}>
								{repo.displayName || repo.name}
							</option>
						))}
					</select>
					<div
						role="group"
						aria-label={t("quickComposer.environment")}
						className="flex rounded-lg border border-border p-1"
					>
						{(["localDirect", "protectedWorktree"] as const).map((value) => (
							<button
								key={value}
								type="button"
								aria-pressed={mode === value}
								disabled={busy || !!unfinished}
								onClick={() =>
									setSelection((s) => ({
										...s,
										modes: { ...s.modes, [s.projectId]: value },
									}))
								}
								className={`rounded-md px-3 py-1 text-xs transition-colors ${mode === value ? "bg-accent text-accent-foreground" : "text-muted-foreground hover:text-foreground"}`}
							>
								{t(`quickComposer.${value}`)}
							</button>
						))}
					</div>
				</div>
				<p className="flex items-center gap-2 text-xs text-muted-foreground">
					<GitBranch className="size-3.5 shrink-0" />
					{t(
						mode === "localDirect"
							? "quickComposer.localHint"
							: "quickComposer.worktreeHint",
					)}
				</p>
				{repository && (
					<p
						title={repository.rootPath}
						className="truncate font-mono text-[11px] text-muted-foreground"
					>
						{repository.rootPath}
					</p>
				)}
				{loading && (
					<p role="status" className="text-xs text-muted-foreground">
						{t("quickComposer.loading")}
					</p>
				)}
				{!loading && !repositories.length && (
					<div className="space-y-2">
						<p className="text-sm text-muted-foreground">
							{t("quickComposer.noProjects")}
						</p>
						<Button
							size="sm"
							variant="outline"
							onClick={() =>
								void openQuickTask().catch((cause) => setError(String(cause)))
							}
						>
							{t("quickComposer.openTask")}
						</Button>
					</div>
				)}
				{!loading && repository && blocked && (
					<p className="text-xs text-destructive">
						{getProviderUnhealthyReason(provider ?? null) ||
							t("quickComposer.selectModel")}
					</p>
				)}
				{error && (
					<p role="alert" className="text-xs text-destructive">
						{error}
					</p>
				)}
				{launch &&
					(launch.phase !== "completed" ||
						(!loading &&
							visibleCompletedLaunchId === launch.id &&
							launch.request.projectId === selection.projectId)) && (
						<div
							role="status"
							className="space-y-2 rounded-lg border border-border bg-muted/30 p-3 text-xs"
						>
							<div className="flex items-center gap-2">
								{busy && <LoaderCircle className="size-3.5 animate-spin" />}
								<span className="min-w-0 flex-1">
									{busy
										? t("quickComposer.starting")
										: t(
												launch.phase === "completed"
													? "quickComposer.started"
													: "quickComposer.interrupted",
											)}
									{launch.phase === "completed" && (
										<span
											className="mt-1 block truncate text-muted-foreground"
											title={launch.request.turn.rawPrompt}
										>
											{launch.request.turn.rawPrompt}
										</span>
									)}
								</span>
								<Button
									size="sm"
									variant="ghost"
									onClick={() =>
										void openQuickTask().catch((cause) =>
											setError(String(cause)),
										)
									}
								>
									{t("quickComposer.openTask")}
									<ArrowUpRight className="ml-1 size-3" />
								</Button>
							</div>
							{launch.error && (
								<p className="text-destructive">{launch.error}</p>
							)}
							{!busy && launch.phase !== "completed" && (
								<details>
									<summary className="cursor-pointer text-muted-foreground">
										{t("quickComposer.savedPrompt")}
									</summary>
									<p className="mt-2 max-h-24 overflow-auto whitespace-pre-wrap">
										{launch.request.turn.rawPrompt}
									</p>
									<Button
										size="sm"
										variant="ghost"
										onClick={() =>
											void navigator.clipboard
												.writeText(launch.request.turn.rawPrompt)
												.then(() => toast.success(t("quickComposer.copied")))
												.catch((cause) => setError(String(cause)))
										}
									>
										{t("quickComposer.copy")}
									</Button>
								</details>
							)}
							{unfinished && !busy && (
								<Button
									size="sm"
									variant="outline"
									onClick={() => void recover()}
								>
									{t("quickComposer.stopRecovery")}
								</Button>
							)}
						</div>
					)}
				<div>
					<WorkspaceComposer
						showExecutionContext={false}
						compactExecutionPicker
						key={selection.projectId}
						draftKey={`quick-composer:${selection.projectId || "new"}`}
						draftSessionId={null}
						disabled={busy || loading || !repository || !!unfinished}
						providerChoices={providers}
						selectedProviderId={selection.providerId}
						selectedModelId={selection.modelId}
						selectedProviderRuntime={providerRuntime}
						sessionSnapshot={null}
						turnQueueEventKey={null}
						pendingPrompt={null}
						focusRequestKey={focusKey}
						workspacePath={null}
						fileMentionRootPath={repository?.rootPath ?? null}
						workspaceBranch={null}
						projectLabel={null}
						currentBranch={null}
						isIsolatedWorkspace={mode === "protectedWorktree"}
						showPlanFollowUpPrompt={false}
						planTitle={null}
						planNeedsInput={false}
						planApproved={false}
						onSelectProvider={(providerId) => {
							const next = providers.find((p) => p.id === providerId);
							setSelection((s) => ({
								...s,
								providerId,
								modelId:
									next?.models.find((m) => m.recommended)?.id ??
									next?.models[0]?.id ??
									"",
							}));
						}}
						onSelectModel={(modelId) =>
							setSelection((s) => ({ ...s, modelId }))
						}
						onSubmitPrompt={submit}
						onAbortSession={() => {}}
						onReviewPlan={() => {}}
					/>
				</div>
				<p className="text-center text-[11px] text-muted-foreground">
					{t("quickComposer.footer")}
				</p>
			</div>
			<Toaster richColors position="top-center" />
		</main>
	);
}
