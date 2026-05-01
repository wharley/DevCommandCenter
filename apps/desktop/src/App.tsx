import { useCallback, useEffect, useMemo, useState, type CSSProperties } from "react";
import {
	ArrowUpRight,
	Command,
	Settings2,
	Sparkles,
} from "lucide-react";
import { Badge } from "./components/ui/badge";
import {
	Card,
	CardContent,
	CardDescription,
	CardFooter,
	CardHeader,
	CardTitle,
} from "./components/ui/card";
import { Button } from "./components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "./components/ui/dropdown-menu";
import { Label } from "./components/ui/label";
import { Separator } from "./components/ui/separator";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "./components/ui/tabs";
import { SIDEBAR_RESIZE_HIT_AREA } from "./shell/layout";
import { useShellPanels } from "./shell/use-panels";
import { useZoom } from "./shell/use-zoom";
import {
	WorkspacesSidebar,
	WorkspaceCommandPalette,
	CreateWorkspaceDialog,
	useWorkspacesPanel,
} from "./features/workspaces";
import {
	SessionWorkbench,
	type RuntimeSessionSnapshot,
} from "./features/sessions/session-workbench";
import { useSessionEventFeed } from "./features/sessions/use-session-event-feed";
import { listProviders } from "./lib/provider-api";
import {
	abortRun,
	resumeSession,
	sendTurn,
	startThread,
} from "./lib/session-api";
import type {
	ProviderCatalog,
} from "@dcc/contracts";

export default function App() {
	useZoom(1);

	const {
		handleResizeKeyDown,
		handleResizeStart,
		sidebarCollapsed,
		sidebarWidth,
		setSidebarCollapsed,
	} = useShellPanels();
	const {
		allWorkspaces,
		createWorkspace,
		filter,
		filteredWorkspaces,
		isCreatingWorkspace,
		selectedWorkspace,
		selectedWorkspaceId,
		showArchived,
		setFilter,
		setSelectedWorkspaceId,
		setShowArchived,
	} = useWorkspacesPanel();
	const [isCommandPaletteOpen, setIsCommandPaletteOpen] = useState(false);
	const [isCreateWorkspaceOpen, setIsCreateWorkspaceOpen] = useState(false);
	const [activeTab, setActiveTab] = useState("overview");
	const { events: sessionEvents } = useSessionEventFeed();
	const [providerCatalog, setProviderCatalog] = useState<ProviderCatalog | null>(
		null,
	);
	const [selectedProviderId, setSelectedProviderId] = useState<string | null>(
		null,
	);
	const [sessionDraft, setSessionDraft] = useState(
		"Bring the Helmor shell density into Dev Command Center, but keep the Tauri + Rust boundary clean.",
	);
	const [sessionSnapshot, setSessionSnapshot] =
		useState<RuntimeSessionSnapshot | null>(null);
	const providerChoices = providerCatalog?.providers ?? [];
	const selectedProvider = useMemo(
		() =>
			providerChoices.find((provider) => provider.id === selectedProviderId) ??
			providerChoices[0] ??
			null,
		[providerChoices, selectedProviderId],
	);

	useEffect(() => {
		let disposed = false;
		void listProviders().then((result) => {
			if (!disposed) {
				setProviderCatalog(result.catalog);
			}
		});

		return () => {
			disposed = true;
		};
	}, []);

	useEffect(() => {
		if (providerChoices.length === 0) {
			return;
		}

		setSelectedProviderId((current) => {
			if (current && providerChoices.some((provider) => provider.id === current)) {
				return current;
			}

			return providerChoices.find((provider) => provider.stable)?.id ?? providerChoices[0].id;
		});
	}, [providerChoices]);

	useEffect(() => {
		setSessionSnapshot(null);
		setSessionDraft(
			"Bring the Helmor shell density into Dev Command Center, but keep the Tauri + Rust boundary clean.",
		);
	}, [selectedWorkspace.id]);

	const handleStartSession = useCallback(async () => {
		if (!selectedProvider) {
			return;
		}

		const result = await startThread({
			workspaceId: selectedWorkspace.id,
			projectId: selectedWorkspace.projectId ?? selectedWorkspace.id,
			providerId: selectedProvider.id,
			title: `${selectedWorkspace.name} session`,
		});

		setSessionSnapshot({
			sessionId: result.session.id,
			projectId: result.session.projectId,
			workspaceId: result.session.workspaceId,
			providerId: result.session.providerId,
			state: result.projection.state,
			turnCount: result.projection.turnCount,
			checkpointCount: result.projection.checkpointCount,
		});
		setActiveTab("runtime");
	}, [selectedProvider, selectedWorkspace.id, selectedWorkspace.name, selectedWorkspace.projectId]);

	const handleSendTurn = useCallback(async () => {
		let currentSession = sessionSnapshot;
		if (!currentSession) {
			if (!selectedProvider) {
				return;
			}

			const started = await startThread({
				workspaceId: selectedWorkspace.id,
				projectId: selectedWorkspace.projectId ?? selectedWorkspace.id,
				providerId: selectedProvider.id,
				title: `${selectedWorkspace.name} session`,
			});

			currentSession = {
				sessionId: started.session.id,
				projectId: started.session.projectId,
				workspaceId: started.session.workspaceId,
				providerId: started.session.providerId,
				state: started.projection.state,
				turnCount: started.projection.turnCount,
				checkpointCount: started.projection.checkpointCount,
			};
			setSessionSnapshot(currentSession);
			setActiveTab("runtime");
		}

		const result = await sendTurn({
			sessionId: currentSession.sessionId,
			prompt: sessionDraft.trim(),
		});

		setSessionSnapshot({
			sessionId: result.session.id,
			projectId: result.session.projectId,
			workspaceId: result.session.workspaceId,
			providerId: result.session.providerId,
			state: result.projection.state,
			turnCount: result.projection.turnCount,
			checkpointCount: result.projection.checkpointCount,
			lastTurnPrompt: result.turn.content,
			lastTurnState: result.turn.state,
		});
		setSessionDraft("");
		setActiveTab("runtime");
	}, [
		selectedProvider,
		selectedWorkspace.id,
		selectedWorkspace.name,
		selectedWorkspace.projectId,
		sessionDraft,
		sessionSnapshot,
	]);

	const handleResumeSession = useCallback(async () => {
		if (!sessionSnapshot) {
			return;
		}

		const result = await resumeSession({ sessionId: sessionSnapshot.sessionId });
		setSessionSnapshot((current) =>
			current
				? {
						...current,
						state: result.projection.state,
						turnCount: result.projection.turnCount,
						checkpointCount: result.projection.checkpointCount,
					}
				: current,
		);
	}, [sessionSnapshot]);

	const handleAbortSession = useCallback(async () => {
		if (!sessionSnapshot) {
			return;
		}

		const result = await abortRun({
			sessionId: sessionSnapshot.sessionId,
			reason: "Stopped from shell",
		});
		setSessionSnapshot((current) =>
			current
				? {
						...current,
						state: result.projection.state,
						turnCount: result.projection.turnCount,
						checkpointCount: result.projection.checkpointCount,
					}
				: current,
		);
	}, [sessionSnapshot]);

	useEffect(() => {
		const onKeyDown = (event: KeyboardEvent) => {
			const isShortcut =
				(event.metaKey || event.ctrlKey) &&
				event.key.toLowerCase() === "k" &&
				!event.shiftKey &&
				!event.altKey;

			if (!isShortcut) {
				return;
			}

			event.preventDefault();
			setIsCommandPaletteOpen(true);
		};

		window.addEventListener("keydown", onKeyDown);
		return () => window.removeEventListener("keydown", onKeyDown);
	}, []);

	const visibleWidth = sidebarCollapsed ? 76 : sidebarWidth;
	const shellStyle = {
		"--dcc-sidebar-width": `${visibleWidth}px`,
	} as CSSProperties;

	return (
		<div className="dcc-shell" style={shellStyle}>
			<aside className="dcc-sidebar">
				<WorkspacesSidebar
					collapsed={sidebarCollapsed}
					filter={filter}
					onFilterChange={setFilter}
					onSelectWorkspace={setSelectedWorkspaceId}
					onShowArchivedChange={setShowArchived}
					onCreateWorkspace={() => setIsCreateWorkspaceOpen(true)}
					onToggleCollapsed={() => setSidebarCollapsed((value) => !value)}
					selectedWorkspaceId={selectedWorkspaceId}
					sidebarWidth={visibleWidth}
					showArchived={showArchived}
					workspaces={filteredWorkspaces}
				/>
			</aside>

			<div
				className="dcc-shell__divider"
				role="separator"
				aria-orientation="vertical"
				aria-label="Resize sidebar"
				tabIndex={0}
				style={{ width: `${SIDEBAR_RESIZE_HIT_AREA}px` }}
				onMouseDown={handleResizeStart("sidebar")}
				onKeyDown={handleResizeKeyDown("sidebar")}
			>
				<div className="dcc-shell__divider-hit" />
			</div>

			<main className="dcc-main">
				<header className="dcc-topbar">
					<div className="dcc-topbar__title">
						<div className="dcc-topbar__meta">
							<Badge>
								<Sparkles />
								Phase 1 shell
							</Badge>
							<Badge variant="outline">Tauri + Rust</Badge>
							<Badge variant="outline">
								{selectedProvider ? selectedProvider.label : "No provider"}
							</Badge>
						</div>
						<h2>Helmor shell density, t3code boundaries, Rust runtime.</h2>
						<p className="dcc-card__description">
							The new shell is the primary path now. The runtime surface is
							being shaped around workspace, provider, and session as the
							first-class units.
						</p>
					</div>
					<div className="dcc-topbar__actions">
						<DropdownMenu>
							<DropdownMenuTrigger asChild>
								<Button type="button" variant="secondary">
									<Settings2 />
									Actions
								</Button>
							</DropdownMenuTrigger>
							<DropdownMenuContent align="end">
								<DropdownMenuItem>Open settings</DropdownMenuItem>
								<DropdownMenuItem>Rebuild contracts</DropdownMenuItem>
								<DropdownMenuSeparator />
							<DropdownMenuItem>Open logs</DropdownMenuItem>
						</DropdownMenuContent>
						</DropdownMenu>
						<Button
							type="button"
							onClick={() => setIsCommandPaletteOpen(true)}
						>
							<Command />
							Cmd+K
						</Button>
					</div>
				</header>
				<div className="dcc-topbar__context">
					<div className="dcc-topbar__context-group">
						<div className="dcc-topbar__context-item">
							<span>Workspace</span>
							<strong>{selectedWorkspace.name}</strong>
						</div>
						<div className="dcc-topbar__context-item">
							<span>Branch</span>
							<strong>{selectedWorkspace.branch}</strong>
						</div>
						<div className="dcc-topbar__context-item">
							<span>Provider</span>
							<strong>{selectedProvider?.label ?? "None"}</strong>
						</div>
						<div className="dcc-topbar__context-item">
							<span>Session</span>
							<strong>{sessionSnapshot?.state ?? "idle"}</strong>
						</div>
					</div>
					<Badge variant={sessionSnapshot ? "success" : "outline"}>
						{sessionSnapshot ? `${sessionSnapshot.turnCount} turns active` : "Ready"}
					</Badge>
				</div>
				<WorkspaceCommandPalette
					open={isCommandPaletteOpen}
					onOpenChange={setIsCommandPaletteOpen}
					workspaces={allWorkspaces}
					selectedWorkspaceId={selectedWorkspace.id}
					onSelectWorkspace={setSelectedWorkspaceId}
				/>
				<CreateWorkspaceDialog
					open={isCreateWorkspaceOpen}
					onOpenChange={setIsCreateWorkspaceOpen}
					onCreateWorkspace={createWorkspace}
					isSubmitting={isCreatingWorkspace}
				/>

				<Separator />

				<Tabs value={activeTab} onValueChange={setActiveTab}>
					<div className="dcc-tabs-bar">
						<TabsList variant="line">
							<TabsTrigger value="overview">Overview</TabsTrigger>
							<TabsTrigger value="runtime">Runtime</TabsTrigger>
						</TabsList>
					</div>

					<TabsContent value="overview">
						<section className="dcc-section-grid">
							<Card className="dcc-card--hero">
								<CardHeader>
									<div>
										<Label>Status</Label>
										<CardTitle>New shell online</CardTitle>
									</div>
									<Badge variant="success">Ready</Badge>
								</CardHeader>
								<CardContent>
									<CardDescription>
										The shell is now the primary path: workspace selection lives
										left, session control lives center, and runtime context stays
										visible without taking over the screen.
									</CardDescription>
									<div className="dcc-card__stat-row">
										<div className="dcc-stat">
											<span>Workspace</span>
											<strong>{selectedWorkspace.id}</strong>
										</div>
										<div className="dcc-stat">
											<span>Branch</span>
											<strong>{selectedWorkspace.branch}</strong>
										</div>
										<div className="dcc-stat">
											<span>Provider</span>
											<strong>{selectedProvider?.label ?? "Unknown"}</strong>
										</div>
									</div>
								</CardContent>
							</Card>

							<Card>
								<CardHeader>
									<div>
										<Label>Contracts</Label>
										<CardTitle>Contract-first shell</CardTitle>
									</div>
								</CardHeader>
								<CardContent>
									<CardDescription>
										Generated bindings will replace handwritten bridge code and
										keep the frontend in sync with the Rust domain.
									</CardDescription>
								</CardContent>
								<CardFooter>
									<Button type="button" variant="ghost">
										<ArrowUpRight />
										View bindings
									</Button>
								</CardFooter>
							</Card>

							<Card>
								<CardHeader>
									<div>
										<Label>Rust core</Label>
										<CardTitle>Boundary first</CardTitle>
									</div>
								</CardHeader>
								<CardContent>
									<CardDescription>
										Domain, ports, and use cases stay isolated from Tauri, with
										the bridge limited to commands and events.
									</CardDescription>
								</CardContent>
								<CardFooter>
									<Button type="button" variant="ghost">
										<ArrowUpRight />
										Inspect crates
									</Button>
								</CardFooter>
							</Card>
						</section>
					</TabsContent>

					<TabsContent value="runtime">
						<SessionWorkbench
							workspaceName={selectedWorkspace.name}
							workspaceBranch={selectedWorkspace.branch}
							selectedProviderLabel={selectedProvider?.label ?? null}
							selectedProviderId={selectedProviderId}
							providerChoices={providerChoices}
							providerCatalog={providerCatalog}
							sessionSnapshot={sessionSnapshot}
							sessionEvents={sessionEvents}
							sessionDraft={sessionDraft}
							onSessionDraftChange={setSessionDraft}
							onSelectProvider={setSelectedProviderId}
							onStartSession={handleStartSession}
							onSendTurn={handleSendTurn}
							onResumeSession={handleResumeSession}
							onAbortSession={handleAbortSession}
							onOpenCommandPalette={() => setIsCommandPaletteOpen(true)}
						/>
					</TabsContent>
				</Tabs>
			</main>
		</div>
	);
}
