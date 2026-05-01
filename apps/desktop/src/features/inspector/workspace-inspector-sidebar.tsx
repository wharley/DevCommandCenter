import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useState, type MouseEvent as ReactMouseEvent, type ReactNode } from "react";
import { toast } from "sonner";
import { BranchToolbar } from "@/components/BranchToolbar";
import { Badge } from "@/components/ui/badge";
import { SessionEventFeed } from "@/features/sessions/session-event-feed";
import type { RuntimeSessionSnapshot } from "@/features/sessions/session-workbench";
import { InspectorChangesSection } from "./inspector-changes-section";
import { GitSectionHeader } from "./git-section-header";
import { resolveCommitMode } from "@/features/commit/WorkspaceCommitButton.logic";
import {
	workspaceGhPrViewWeb,
	workspaceGitCommitPush,
	workspaceGitPush,
} from "@/lib/workspace-api";
import { useWorkspaceGitStatus, WORKSPACE_GIT_STATUS_QUERY_KEY } from "./use-workspace-git-status";
import { EmptyState } from "@/features/panel";
import { cn } from "@/lib/utils";
import type { CoreEvent, ProviderCatalog } from "@dcc/contracts";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { getProviderChips, summarizeProviderHealth } from "@/features/providers/provider-display";

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
};

const MIN_SECTION_HEIGHT = 128;
const MAX_SECTION_HEIGHT = 360;
const INITIAL_CHANGES_HEIGHT = 200;

type InspectorTab = "activity" | "context";

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

function gitSectionHeaderHighlightClass(mode: ReturnType<typeof resolveCommitMode>) {
	switch (mode) {
		case "fix":
		case "closed":
			return "bg-[var(--workspace-pr-closed-header-bg)]";
		case "resolve-conflicts":
			return "bg-[var(--workspace-pr-conflicts-header-bg)]";
		case "merge":
		case "open-pr":
			return "bg-[var(--workspace-pr-open-header-bg)]";
		case "merged":
			return "bg-[var(--workspace-pr-merged-header-bg)]";
		default:
			return "bg-muted/25";
	}
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
}: WorkspaceInspectorSidebarProps) {
	const hasWorkspace = Boolean(workspaceId && workspaceName && workspaceBranch);
	const pathLine =
		workspacePath && workspacePath.length > 0
			? workspacePath.length > 56
				? `…${workspacePath.slice(-55)}`
				: workspacePath
			: null;
	const commitMode = resolveCommitMode(workspaceBranch ?? "");
	const headerToneClass = gitSectionHeaderHighlightClass(commitMode);
	const queryClient = useQueryClient();
	const gitStatusQuery = useWorkspaceGitStatus(workspacePath);

	const handleInspectorCommit = useCallback(async () => {
		const root = workspacePath?.trim();
		if (!root) {
			toast.error("No workspace path");
			throw new Error("No workspace path");
		}

		switch (commitMode) {
			case "merged":
			case "closed":
				return;
			case "push":
				await workspaceGitPush({ workspaceRoot: root });
				toast.success("Pushed");
				break;
			case "open-pr":
				await workspaceGhPrViewWeb({ workspaceRoot: root });
				toast.success("Opened PR in browser");
				break;
			default: {
				const message = window.prompt("Commit message", "dcc: checkpoint");
				if (message === null) {
					return;
				}
				const trimmed = message.trim();
				if (!trimmed) {
					toast.error("Commit message required");
					throw new Error("Commit message required");
				}
				await workspaceGitCommitPush({ workspaceRoot: root, message: trimmed });
				toast.success("Committed and pushed");
				break;
			}
		}

		await queryClient.invalidateQueries({
			queryKey: [WORKSPACE_GIT_STATUS_QUERY_KEY, root],
		});
	}, [commitMode, queryClient, workspacePath]);

	const [changesHeight, setChangesHeight] = useState(INITIAL_CHANGES_HEIGHT);
	const [inspectorTab, setInspectorTab] = useState<InspectorTab>("activity");

	function handleResizeGitSectionStart(event: ReactMouseEvent<HTMLButtonElement>) {
		event.preventDefault();
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
			<div className="dcc-inspector flex h-full min-h-0 flex-col overflow-hidden text-foreground">
				<div className="flex min-h-0 flex-1 items-center justify-center px-4 py-6">
					<EmptyState
						title="No workspace selected"
						description="Open or create a workspace to inspect git state and session activity."
					/>
				</div>
			</div>
		);
	}

	const activityCount = sessionEvents.length;
	const catalogCount = providerCatalog?.providers.length ?? 0;

	return (
		<div className="dcc-inspector flex h-full min-h-0 flex-col overflow-hidden text-foreground" data-dcc-inspector-root>
			<section
				className={cn(
					"flex shrink-0 flex-col overflow-hidden border-b border-border/60",
					headerToneClass,
				)}
				style={{ height: `${changesHeight}px` }}
			>
				<GitSectionHeader
					workspaceDisplayName={workspaceName ?? "Workspace"}
					commitMode={commitMode}
					isRefreshing={gitStatusQuery.isFetching && !gitStatusQuery.isPending}
					onCommit={handleInspectorCommit}
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
						<InspectorChangesSection workspaceRoot={workspacePath} />
					</div>
				</div>
			</section>

			<ResizeHandle label="Resize Git section" onMouseDown={handleResizeGitSectionStart} />

			<section className="flex min-h-0 flex-1 flex-col overflow-hidden border-t border-border/40">
				<Tabs
					value={inspectorTab}
					onValueChange={(value) => {
						if (value === "activity" || value === "context") {
							setInspectorTab(value);
						}
					}}
					className="flex min-h-0 flex-1 flex-col gap-0"
				>
					<div className="flex shrink-0 items-center justify-between gap-2 border-b border-border/40 px-3 py-2">
						<div className="min-w-0">
							<p className="text-[11px] font-medium uppercase tracking-[0.08em] text-muted-foreground">
								Session
							</p>
							<p className="truncate text-[13px] font-medium leading-tight text-foreground">
								Activity and workspace context
							</p>
						</div>
						<Badge variant="outline" className="h-6 shrink-0 px-2 text-[10px] font-normal">
							{activityCount} events
						</Badge>
					</div>

					<div className="shrink-0 border-b border-border/40 bg-muted/15 px-2">
						<TabsList variant="line" className="h-9 w-full justify-start gap-0 border-0 bg-transparent p-0">
							<TabsTrigger value="activity" className="h-9 rounded-none px-3 text-[12px]">
								Activity
							</TabsTrigger>
							<TabsTrigger value="context" className="h-9 rounded-none px-3 text-[12px]">
								Context
								{catalogCount > 0 ? (
									<span className="ml-1.5 tabular-nums text-[10px] text-muted-foreground">
										({catalogCount})
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
									Workspace
								</p>
								<div className="rounded-md border border-border/50 bg-muted/10 px-2">
									<DetailRow label="Name">{workspaceName ?? "—"}</DetailRow>
									<DetailRow label="Id">{workspaceId ?? "—"}</DetailRow>
									<DetailRow label="Branch">{workspaceBranch ?? "—"}</DetailRow>
									<DetailRow label="Path">
										<span title={workspacePath ?? undefined}>{workspacePath ?? "—"}</span>
									</DetailRow>
								</div>
							</div>

							<div>
								<p className="mb-2 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
									Composer selection
								</p>
								<div className="rounded-md border border-border/50 bg-muted/10 px-2">
									<DetailRow label="Provider">{selectedProviderLabel ?? "—"}</DetailRow>
									<DetailRow label="Model">{selectedModelLabel ?? "—"}</DetailRow>
								</div>
							</div>

							<div>
								<p className="mb-2 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
									Session runtime
								</p>
								<div className="rounded-md border border-border/50 bg-muted/10 px-2">
									<DetailRow label="State">
										<span className="inline-flex items-center gap-2">
											{sessionState}
											{sessionId ? (
												<Badge variant="outline" className="font-mono text-[10px] font-normal">
													{sessionId.length > 14 ? `${sessionId.slice(0, 12)}…` : sessionId}
												</Badge>
											) : null}
										</span>
									</DetailRow>
									{sessionSnapshot ? (
										<>
											<DetailRow label="Turns">{String(sessionSnapshot.turnCount)}</DetailRow>
											<DetailRow label="Checkpoints">{String(sessionSnapshot.checkpointCount)}</DetailRow>
											<DetailRow label="Last turn">
												{sessionSnapshot.lastTurnPrompt ?? "—"}
											</DetailRow>
											<DetailRow label="Provider id">{sessionSnapshot.providerId}</DetailRow>
										</>
									) : (
										<DetailRow label="Session">No active session — start one from the composer.</DetailRow>
									)}
								</div>
							</div>

							<div>
								<div className="mb-2 flex items-center justify-between gap-2">
									<p className="text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
										Provider catalog
									</p>
									{catalogCount > 0 ? (
										<Badge variant="secondary" className="h-5 text-[10px] font-normal">
											{catalogCount} registered
										</Badge>
									) : null}
								</div>
								<ProviderCatalogDense catalog={providerCatalog} />
							</div>

							<p className="text-[10px] leading-relaxed text-muted-foreground">
								Terminal sessions stay in the workbench drawer at the bottom of the layout — they are not
								mirrored here so the inspector stays focused on Git and session signals.
							</p>
						</div>
					</TabsContent>
				</Tabs>
			</section>
		</div>
	);
}
