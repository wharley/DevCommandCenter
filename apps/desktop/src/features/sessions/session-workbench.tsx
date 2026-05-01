import { useState } from "react";
import { ArrowUpRight, Command, TerminalSquare } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { CardDescription } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Separator } from "@/components/ui/separator";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import { Textarea } from "@/components/ui/textarea";
import { WorkspaceTerminalDrawer } from "@/features/terminal";
import type { CoreEvent, ProviderCatalog } from "@dcc/contracts";
import { SessionEventFeed } from "./session-event-feed";

export type RuntimeSessionSnapshot = {
	sessionId: string;
	projectId: string;
	workspaceId: string;
	providerId: string;
	state: string;
	turnCount: number;
	checkpointCount: number;
	lastTurnPrompt?: string | null;
	lastTurnState?: string | null;
};

type SessionWorkbenchProps = {
	workspaceId: string;
	workspaceName: string;
	workspaceBranch: string;
	workspacePath: string | null;
	selectedProviderLabel: string | null;
	selectedProviderId: string | null;
	providerChoices: ProviderCatalog["providers"];
	sessionSnapshot: RuntimeSessionSnapshot | null;
	sessionEvents: CoreEvent[];
	sessionDraft: string;
	onSessionDraftChange: (value: string) => void;
	onSelectProvider: (providerId: string) => void;
	onStartSession: () => void;
	onSendTurn: () => void;
	onResumeSession: () => void;
	onAbortSession: () => void;
	onOpenCommandPalette: () => void;
};

function shortPath(path: string | null, max = 48) {
	if (!path) {
		return null;
	}
	if (path.length <= max) {
		return path;
	}
	return `…${path.slice(-(max - 1))}`;
}

export function SessionWorkbench({
	workspaceId,
	workspaceName,
	workspaceBranch,
	workspacePath,
	selectedProviderLabel,
	selectedProviderId,
	providerChoices,
	sessionSnapshot,
	sessionEvents,
	sessionDraft,
	onSessionDraftChange,
	onSelectProvider,
	onStartSession,
	onSendTurn,
	onResumeSession,
	onAbortSession,
	onOpenCommandPalette,
}: SessionWorkbenchProps) {
	const [terminalDrawerOpen, setTerminalDrawerOpen] = useState(false);
	const pathShort = shortPath(workspacePath);
	const terminalAvailable = Boolean(workspacePath);
	const sessionState = sessionSnapshot?.state ?? "idle";
	const sessionId = sessionSnapshot?.sessionId ?? null;
	const headline =
		workspaceBranch && pathShort
			? `${workspaceBranch} · ${pathShort}`
			: workspaceBranch || pathShort || "Workspace";

	return (
		<section className="dcc-runtime-workbench dcc-runtime-workbench--session flex min-h-0 flex-1 flex-col">
			{/* Compact primary bar (t3code ChatHeader) + session actions */}
			<header className="dcc-runtime-workbench__header shrink-0 gap-3 pb-3">
				<div className="flex min-w-0 flex-1 items-center gap-2 overflow-hidden sm:gap-3">
					<h2
						className="m-0 min-w-0 shrink truncate text-sm font-medium tracking-tight text-foreground"
						title={workspaceName}
					>
						{workspaceName}
					</h2>
					{workspaceBranch ? (
						<Badge variant="outline" className="hidden shrink-0 sm:inline-flex">
							<span className="max-w-[9rem] truncate font-normal">{workspaceBranch}</span>
						</Badge>
					) : null}
					{pathShort ? (
						<span
							className="hidden max-w-[14rem] truncate text-[11px] text-muted-foreground md:inline"
							title={workspacePath ?? undefined}
						>
							{pathShort}
						</span>
					) : null}
				</div>
				<div className="dcc-runtime-workbench__header-actions">
					{sessionSnapshot ? (
						<Badge variant="success" className="font-normal">
							{sessionSnapshot.state}
						</Badge>
					) : (
						<Badge variant="outline" className="font-normal">
							idle
						</Badge>
					)}
					<Button
						type="button"
						variant="secondary"
						size="sm"
						onClick={onResumeSession}
						disabled={!sessionSnapshot}
					>
						Resume
					</Button>
					<Button
						type="button"
						variant="secondary"
						size="sm"
						onClick={onAbortSession}
						disabled={!sessionSnapshot}
					>
						Abort
					</Button>
					<Tooltip>
						<TooltipTrigger asChild>
							<Button
								type="button"
								variant={terminalDrawerOpen ? "secondary" : "outline"}
								size="icon"
								className="size-8 shrink-0"
								aria-label={
									terminalDrawerOpen
										? "Hide terminal drawer"
										: "Show terminal drawer"
								}
								disabled={!terminalAvailable}
								aria-pressed={terminalDrawerOpen}
								onClick={() => setTerminalDrawerOpen((current) => !current)}
							>
								<TerminalSquare className="size-3.5" />
							</Button>
						</TooltipTrigger>
						<TooltipContent side="bottom">
							{!terminalAvailable
								? "Terminal needs a workspace path."
								: terminalDrawerOpen
									? "Hide terminal drawer (Esc)"
									: "Toggle terminal drawer"}
						</TooltipContent>
					</Tooltip>
					<Button
						type="button"
						variant="ghost"
						size="sm"
						onClick={onOpenCommandPalette}
						className="gap-1.5 text-muted-foreground hover:text-foreground"
					>
						<Command className="size-3.5" />
						<span className="hidden sm:inline">Workspaces</span>
					</Button>
				</div>
			</header>

			<p className="m-0 hidden pb-3 text-[12px] leading-snug text-muted-foreground sm:block">
				{headline}
			</p>

			<Separator className="shrink-0 bg-border opacity-70" />

			{/* Helmor-style contextual toolbar + provider pills (t3 secondary row) */}
			<div className="dcc-runtime-workbench__toolbar dcc-runtime-workbench__toolbar--providers mt-4 shrink-0">
				<div className="dcc-runtime-workbench__toolbar-meta min-w-0">
					<span>Runtime</span>
					<strong>Providers</strong>
				</div>
				<div className="dcc-runtime-workbench__toolbar-actions min-w-0 flex-1">
					<div className="dcc-runtime-workbench__provider-strip">
						{providerChoices.map((provider) => (
							<Button
								key={provider.id}
								type="button"
								variant={provider.id === selectedProviderId ? "default" : "outline"}
								size="sm"
								className="h-8 rounded-full px-3 text-[12px] font-medium"
								onClick={() => onSelectProvider(provider.id)}
							>
								{provider.label}
							</Button>
						))}
					</div>
					{providerChoices.length === 0 ? (
						<span className="self-center text-[12px] text-muted-foreground">
							No providers configured.
						</span>
					) : selectedProviderLabel ? (
						<span className="self-center whitespace-nowrap text-[12px] text-muted-foreground">
							Active · {selectedProviderLabel}
						</span>
					) : null}
				</div>
			</div>

			<div className="flex min-h-0 flex-1 flex-col gap-4 pt-4">
				<div
					className="dcc-runtime-workbench__timeline-shell dcc-runtime-workbench__timeline-shell--conversation dcc-runtime-workbench__timeline-shell--fill flex min-h-0 min-w-0 flex-1 flex-col"
					aria-label="Session activity timeline"
				>
					<SessionEventFeed events={sessionEvents} compact />
				</div>

				<div className="dcc-runtime-workbench__composer shrink-0">
				<div className="dcc-runtime-workbench__composer-top">
					<div>
						<Label htmlFor="dcc-session-draft" className="text-[13px] font-medium">
							Message
						</Label>
						<CardDescription className="mt-0.5 max-w-[44rem] text-[12px]">
							Next turn runs on the Dev Command Center session bridge for the selected
							provider.
						</CardDescription>
					</div>
				</div>
				<Textarea
					id="dcc-session-draft"
					rows={4}
					value={sessionDraft}
					onChange={(event) => onSessionDraftChange(event.target.value)}
					placeholder="Ask for a change, @mention paths, or describe the task…"
					className="w-full resize-y border-border bg-background/80 text-[13px]"
				/>
				<div className="dcc-runtime-workbench__composer-footer">
					<div className="dcc-runtime-workbench__composer-meta">
						{sessionSnapshot?.sessionId ? (
							<span className="text-[11px] tabular-nums text-muted-foreground">
								{sessionSnapshot.turnCount} turns · {sessionSnapshot.checkpointCount}{" "}
								checkpoints
							</span>
						) : (
							<span className="text-[11px] text-muted-foreground">No active session</span>
						)}
					</div>
					<div className="dcc-runtime-workbench__composer-actions">
						<Button type="button" onClick={onStartSession} variant="secondary" size="sm">
							Start session
						</Button>
						<Button type="button" onClick={onSendTurn} size="sm" className="gap-1.5">
							<ArrowUpRight className="size-4" />
							Send
						</Button>
					</div>
				</div>
			</div>

				<WorkspaceTerminalDrawer
					open={terminalDrawerOpen}
					onOpenChange={setTerminalDrawerOpen}
					workspaceId={workspaceId}
					workspaceName={workspaceName}
					workspaceBranch={workspaceBranch}
					workspacePath={workspacePath}
					providerLabel={selectedProviderLabel}
					sessionState={sessionState}
					sessionId={sessionId}
				/>
			</div>
		</section>
	);
}
