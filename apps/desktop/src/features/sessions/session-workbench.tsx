import { useState } from "react";
import { ArrowUpRight } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { WorkspaceTerminalDrawer } from "@/features/terminal";
import { cn } from "@/lib/utils";
import type { CoreEvent, ProviderCatalog } from "@dcc/contracts";
import { DccWorkbenchChatHeader } from "./dcc-workbench-chat-header";
import { SessionEventFeed } from "./session-event-feed";
import type { DccRuntimeSessionSnapshot } from "./workbench-types";

export type RuntimeSessionSnapshot = DccRuntimeSessionSnapshot;

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

function shortPath(path: string | null, max = 52) {
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
	const pathCaption = shortPath(workspacePath);
	const terminalAvailable = Boolean(workspacePath);
	const sessionState = sessionSnapshot?.state ?? "idle";
	const sessionId = sessionSnapshot?.sessionId ?? null;
	const isGitRepo = Boolean(workspaceBranch) || Boolean(workspacePath);

	const threadTitle = workspaceName;
	const projectBadgeLabel = workspaceBranch || null;

	return (
		<div className="@container/header-actions flex min-h-0 min-w-0 flex-1 flex-col overflow-x-hidden bg-background">
			<header
				className={cn(
					"border-b border-border pb-2 pt-2 sm:pb-3 sm:pt-3",
					"pl-[calc(env(safe-area-inset-left)+0.75rem)] pr-[calc(env(safe-area-inset-right)+0.75rem)]",
					"sm:pl-[calc(env(safe-area-inset-left)+1.25rem)] sm:pr-[calc(env(safe-area-inset-right)+1.25rem)]",
				)}
			>
				<DccWorkbenchChatHeader
					threadTitle={threadTitle}
					projectBadgeLabel={projectBadgeLabel}
					isGitRepo={isGitRepo}
					pathCaption={pathCaption}
					sessionSnapshot={sessionSnapshot}
					terminalAvailable={terminalAvailable}
					terminalOpen={terminalDrawerOpen}
					onToggleTerminal={() =>
						setTerminalDrawerOpen((current) => !current)
					}
					onOpenCommandPalette={onOpenCommandPalette}
					onResumeSession={onResumeSession}
					onAbortSession={onAbortSession}
				/>
			</header>

			<div className="flex min-h-0 min-w-0 flex-1 flex-col">
				<div className="relative flex min-h-0 min-w-0 flex-1 flex-col">
					<SessionEventFeed events={sessionEvents} compact />
				</div>

				<div
					className={cn(
						"pl-[calc(env(safe-area-inset-left)+0.75rem)] pr-[calc(env(safe-area-inset-right)+0.75rem)] pt-1.5",
						"sm:pl-[calc(env(safe-area-inset-left)+1.25rem)] sm:pr-[calc(env(safe-area-inset-right)+1.25rem)] sm:pt-2",
						isGitRepo
							? "pb-[calc(env(safe-area-inset-bottom)+0.25rem)]"
							: "pb-[calc(env(safe-area-inset-bottom)+0.75rem)] sm:pb-[calc(env(safe-area-inset-bottom)+1rem)]",
					)}
				>
					<div className="mx-auto w-full min-w-0 max-w-[52rem]">
						<div className="group rounded-[22px] bg-gradient-to-b from-border/65 to-border/35 p-px transition-colors duration-200">
							<div
								className={cn(
									"rounded-[20px] border border-border bg-card transition-colors duration-200",
									"has-focus-visible:border-ring/45",
								)}
							>
								<div className="relative px-3 pb-2 pt-2.5 sm:px-4 sm:pb-3 sm:pt-3">
									<div className="scrollbar-none mb-2.5 flex max-w-full gap-2 overflow-x-auto pb-0.5 sm:gap-2.5">
										<span className="shrink-0 pt-1 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
											Model
										</span>
										<div className="flex shrink-0 flex-nowrap gap-1.5">
											{providerChoices.map((provider) => (
												<Button
													key={provider.id}
													type="button"
													variant={
														provider.id === selectedProviderId
															? "default"
															: "outline"
													}
													size="sm"
													className="h-7 shrink-0 rounded-full px-2.5 text-[11px] font-medium leading-none"
													onClick={() => onSelectProvider(provider.id)}
												>
													{provider.label}
												</Button>
											))}
										</div>
										{providerChoices.length === 0 ? (
											<span className="truncate text-[11px] text-muted-foreground">
												No providers configured
											</span>
										) : selectedProviderLabel ? (
											<span className="truncate text-[11px] text-muted-foreground">
												{selectedProviderLabel}
											</span>
										) : null}
									</div>

									<div className="flex flex-wrap items-start justify-between gap-2 pb-2">
										<div className="min-w-0">
											<Label
												htmlFor="dcc-session-draft"
												className="text-[13px] font-medium leading-none"
											>
												Message
											</Label>
											<p className="mt-1 max-w-[40rem] text-[12px] leading-snug text-muted-foreground">
												Send the next workspace turn via the desktop session bridge.
											</p>
										</div>
										<div className="flex w-full shrink-0 flex-wrap items-center gap-2 sm:w-auto lg:hidden">
											<Button
												type="button"
												variant="outline"
												size="sm"
												className="h-9 flex-1 text-[11px]"
												onClick={onResumeSession}
												disabled={!sessionSnapshot}
											>
												Resume
											</Button>
											<Button
												type="button"
												variant="outline"
												size="sm"
												className="h-9 flex-1 text-[11px]"
												onClick={onAbortSession}
												disabled={!sessionSnapshot}
											>
												Abort
											</Button>
											<Badge
												variant="outline"
												className="h-9 shrink-0 px-2 py-0 text-[11px] font-normal tabular-nums"
											>
												{sessionSnapshot?.state ?? "idle"}
											</Badge>
										</div>
									</div>

									<Textarea
										id="dcc-session-draft"
										rows={3}
										value={sessionDraft}
										onChange={(event) =>
											onSessionDraftChange(event.target.value)
										}
										placeholder="Ask for a change, @ paths, or describe the task…"
										className="min-h-[7.25rem] w-full resize-y rounded-xl border-border bg-background/80 px-3 py-2.5 text-[13px] leading-relaxed placeholder:text-muted-foreground/75"
									/>

									<div className="mt-2 flex flex-wrap items-center justify-between gap-2 pt-1">
										<div className="tabular-nums text-[11px] text-muted-foreground">
											{sessionSnapshot?.sessionId ? (
												<>
													{sessionSnapshot.turnCount} turns ·{" "}
													{sessionSnapshot.checkpointCount} checkpoints
												</>
											) : (
												"No active session"
											)}
										</div>
										<div className="flex flex-wrap items-center justify-end gap-2">
											<Button
												type="button"
												onClick={onStartSession}
												variant="secondary"
												size="sm"
												className="h-9"
											>
												Start session
											</Button>
											<Button
												type="button"
												onClick={onSendTurn}
												size="sm"
												className="h-9 gap-1.5 px-4"
											>
												<ArrowUpRight className="size-4" strokeWidth={2} />
												Send
											</Button>
										</div>
									</div>
								</div>
							</div>
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
		</div>
	);
}
