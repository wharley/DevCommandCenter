import { ArrowUpRight, Command, Settings2 } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Separator } from "@/components/ui/separator";
import { Textarea } from "@/components/ui/textarea";
import { ProviderCatalogCard } from "@/features/providers/provider-catalog-card";
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
	workspaceName: string;
	workspaceBranch: string;
	selectedProviderLabel: string | null;
	selectedProviderId: string | null;
	providerChoices: ProviderCatalog["providers"];
	providerCatalog: ProviderCatalog | null;
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

export function SessionWorkbench({
	workspaceName,
	workspaceBranch,
	selectedProviderLabel,
	selectedProviderId,
	providerChoices,
	providerCatalog,
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
	return (
		<section className="dcc-runtime-workbench">
			<Card className="dcc-runtime-workbench__main">
				<CardHeader className="dcc-runtime-workbench__header">
					<div className="dcc-runtime-workbench__hero">
						<div>
							<Label>Session</Label>
							<CardTitle>Workspace cockpit</CardTitle>
							<CardDescription>
								{workspaceName} · {workspaceBranch}
							</CardDescription>
						</div>
						<div className="dcc-runtime-workbench__hero-status">
							<Badge variant={sessionSnapshot ? "success" : "outline"}>
								{sessionSnapshot ? sessionSnapshot.state : "idle"}
							</Badge>
							<Badge variant="outline">
								{selectedProviderLabel ?? "Select provider"}
							</Badge>
						</div>
					</div>
					<div className="dcc-runtime-workbench__header-chips">
						<Badge variant="outline">Workspace {sessionSnapshot?.workspaceId ?? "pending"}</Badge>
						<Badge variant="outline">Provider {selectedProviderLabel ?? "Select one"}</Badge>
						<Badge variant="outline">Turns {sessionSnapshot?.turnCount ?? 0}</Badge>
						<Badge variant="outline">Checkpoints {sessionSnapshot?.checkpointCount ?? 0}</Badge>
					</div>
				</CardHeader>

				<CardContent className="dcc-runtime-workbench__content">
					<div className="dcc-runtime-workbench__toolbar">
						<div className="dcc-runtime-workbench__toolbar-meta">
							<span>Runtime surface</span>
							<strong>{sessionSnapshot?.sessionId ?? "No session started"}</strong>
						</div>
						<div className="dcc-runtime-workbench__toolbar-actions">
							<Button type="button" variant="secondary" onClick={onResumeSession} disabled={!sessionSnapshot}>
								Resume
							</Button>
							<Button type="button" variant="secondary" onClick={onAbortSession} disabled={!sessionSnapshot}>
								Abort
							</Button>
							<Button type="button" variant="ghost" onClick={onOpenCommandPalette}>
								<Command />
								Workspaces
							</Button>
							<Button type="button" onClick={onStartSession}>
								Start session
							</Button>
						</div>
					</div>

					<div className="dcc-runtime-workbench__timeline-shell">
						<SessionEventFeed events={sessionEvents} compact />
					</div>

					<div className="dcc-runtime-workbench__composer">
						<div className="dcc-runtime-workbench__composer-top">
							<div>
								<Label htmlFor="dcc-session-draft">Prompt</Label>
								<CardDescription>
									Send the next turn into the current provider session.
								</CardDescription>
							</div>
							<Button type="button" variant="outline" onClick={onSendTurn}>
								<ArrowUpRight />
								Send turn
							</Button>
						</div>
						<Textarea
							id="dcc-session-draft"
							rows={7}
							value={sessionDraft}
							onChange={(event) => onSessionDraftChange(event.target.value)}
							placeholder="Describe the workspace change or agent task here..."
						/>
					</div>
				</CardContent>
			</Card>

			<div className="dcc-runtime-workbench__rail">
				<ProviderCatalogCard catalog={providerCatalog} />
				<Card className="dcc-session-state-card">
					<CardHeader>
						<div className="dcc-card__meta-row">
							<CardTitle>Session state</CardTitle>
							<Badge variant="outline">
								{sessionSnapshot?.lastTurnState ?? "pending"}
							</Badge>
						</div>
					</CardHeader>
					<CardContent className="dcc-runtime-feed__content">
						{sessionSnapshot ? (
							<div className="dcc-runtime-feed__list">
								<div className="dcc-runtime-feed__row">
									<strong>Projection</strong>
									<small>
										turns {sessionSnapshot.turnCount} · checkpoints{" "}
										{sessionSnapshot.checkpointCount}
									</small>
								</div>
								<div className="dcc-runtime-feed__row">
									<strong>Provider</strong>
									<small>{sessionSnapshot.providerId}</small>
								</div>
								<div className="dcc-runtime-feed__row">
									<strong>Last turn</strong>
									<small>
										{sessionSnapshot.lastTurnPrompt ?? "No turn yet"}
									</small>
								</div>
							</div>
						) : (
							<p className="dcc-card__description">
								No session started yet. Start one to see the shell and Rust core
								move together.
							</p>
						)}
					</CardContent>
				</Card>
				<Card className="dcc-session-actions-card">
					<CardHeader>
						<div className="dcc-card__meta-row">
							<CardTitle>Providers</CardTitle>
							<Badge variant="outline">{providerChoices.length} available</Badge>
						</div>
					</CardHeader>
					<CardContent className="dcc-session-actions-card__content">
						<div className="dcc-provider-pills">
							{providerChoices.map((provider) => (
								<Button
									key={provider.id}
									type="button"
									variant={provider.id === selectedProviderId ? "default" : "secondary"}
									onClick={() => onSelectProvider(provider.id)}
								>
									{provider.label}
								</Button>
							))}
						</div>
						<Separator />
						<p className="dcc-card__description">
							{selectedProviderLabel
								? `${selectedProviderLabel} is the active provider for the next turn.`
								: "Select a provider before starting a session."}
						</p>
						<Button type="button" variant="ghost" onClick={onOpenCommandPalette}>
							<Settings2 />
							Open workspace command palette
						</Button>
					</CardContent>
				</Card>
			</div>
		</section>
	);
}
