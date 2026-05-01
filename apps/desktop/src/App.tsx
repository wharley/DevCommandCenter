import { useEffect, useState, type CSSProperties } from "react";
import {
	ArrowUpRight,
	Command,
	Settings2,
	Sparkles,
} from "lucide-react";
import { Badge } from "./components/ui/badge";
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
import { Textarea } from "./components/ui/textarea";
import { SIDEBAR_RESIZE_HIT_AREA } from "./shell/layout";
import { useShellPanels } from "./shell/use-panels";
import { useZoom } from "./shell/use-zoom";
import {
	WorkspacesSidebar,
	WorkspaceCommandPalette,
	useWorkspacesPanel,
} from "./features/workspaces";

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
		filter,
		filteredWorkspaces,
		selectedWorkspace,
		selectedWorkspaceId,
		setFilter,
		setSelectedWorkspaceId,
	} = useWorkspacesPanel();
	const [isCommandPaletteOpen, setIsCommandPaletteOpen] = useState(false);

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
					onToggleCollapsed={() => setSidebarCollapsed((value) => !value)}
					selectedWorkspaceId={selectedWorkspaceId}
					sidebarWidth={visibleWidth}
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
						</div>
						<h2>Helmor shell, t3code contracts, Rust core.</h2>
						<p className="dcc-card__description">
							The new shell is the primary path now. The legacy app remains
							preserved while we move UI primitives, feature folders, and the
							contract surface into the monorepo.
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
				<WorkspaceCommandPalette
					open={isCommandPaletteOpen}
					onOpenChange={setIsCommandPaletteOpen}
					workspaces={allWorkspaces}
					selectedWorkspaceId={selectedWorkspace.id}
					onSelectWorkspace={setSelectedWorkspaceId}
				/>

				<Separator />

				<section className="dcc-section-grid">
					<article className="dcc-card dcc-card--hero">
						<div className="dcc-card__header">
							<div>
								<Label>Status</Label>
								<h3>New shell online</h3>
							</div>
							<Badge variant="success">Ready</Badge>
						</div>
						<p className="dcc-card__description">
							This shell is intentionally thin. The heavy lifting moves into Rust
							and the contract layer as the migration advances.
						</p>
						<div className="dcc-card__stat-row">
							<div className="dcc-stat">
								<span>Workspace</span>
								<strong>{selectedWorkspace.id}</strong>
							</div>
							<div className="dcc-stat">
								<span>Branch</span>
								<strong>{selectedWorkspace.branch}</strong>
							</div>
						</div>
					</article>

					<article className="dcc-card">
						<Label>Contracts</Label>
						<p className="dcc-card__description">
							Generated bindings will replace handwritten bridge code and keep the
							frontend in sync with the Rust domain.
						</p>
						<Button type="button" variant="ghost">
							<ArrowUpRight />
							View bindings
						</Button>
					</article>

					<article className="dcc-card">
						<Label>Rust core</Label>
						<p className="dcc-card__description">
							Domain, ports, and use cases stay isolated from Tauri, with the
							bridge limited to commands and events.
						</p>
						<Button type="button" variant="ghost">
							<ArrowUpRight />
							Inspect crates
						</Button>
					</article>
				</section>

				<section className="dcc-dual-grid">
					<article className="dcc-card dcc-terminal">
						<div className="dcc-card__header">
							<div>
								<Label>Terminal</Label>
								<h3>Runtime stream</h3>
							</div>
							<Badge variant="outline">Stub</Badge>
						</div>
						<div className="dcc-terminal__viewport">
							<p className="dcc-card__description">
								Agent output, git progress, and Rust events will flow here once the
								infra adapters land.
							</p>
							<Button type="button" variant="secondary">
								<ArrowUpRight />
								Open terminal
							</Button>
						</div>
					</article>

					<article className="dcc-card dcc-composer">
						<div className="dcc-card__header">
							<div>
								<Label>Composer</Label>
								<h3>Seed the next use case</h3>
							</div>
							<Badge variant="outline">Draft</Badge>
						</div>
						<Textarea
							rows={8}
							placeholder="Describe the workspace change or agent task here..."
						/>
						<div className="dcc-topbar__actions">
							<Button type="button" variant="secondary">
								Cancel
							</Button>
							<Button type="button">Send</Button>
						</div>
					</article>
				</section>
			</main>
		</div>
	);
}
