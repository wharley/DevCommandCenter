import { DccWorkbenchChatHeader } from "@/features/sessions/dcc-workbench-chat-header";
import type { RuntimeSessionSnapshot } from "@/features/sessions/workbench-types";
import type { ProviderCatalog } from "@dcc/contracts";
import type { CoreEvent } from "@dcc/contracts";
import { ActiveThreadViewport } from "./ActiveThreadViewport";
import { WorkspaceComposer } from "@/features/composer";

type WorkspacePanelProps = {
	workspaceId: string;
	workspaceName: string;
	workspaceBranch: string;
	workspacePath: string | null;
	selectedProviderLabel: string | null;
	selectedProviderId: string | null;
	providerChoices: ProviderCatalog["providers"];
	sessionSnapshot: RuntimeSessionSnapshot | null;
	sessionEvents: CoreEvent[];
	onSelectProvider: (providerId: string) => void;
	onStartSession: () => void;
	onSubmitPrompt: (prompt: string) => Promise<void>;
	onResumeSession: () => void;
	onAbortSession: () => void;
	onOpenCommandPalette: () => void;
	terminalAvailable: boolean;
	terminalOpen: boolean;
	onToggleTerminal: () => void;
};

export function WorkspacePanel({
	workspaceId,
	workspaceName,
	workspaceBranch,
	workspacePath,
	selectedProviderLabel,
	selectedProviderId,
	providerChoices,
	sessionSnapshot,
	sessionEvents,
	onSelectProvider,
	onStartSession,
	onSubmitPrompt,
	onResumeSession,
	onAbortSession,
	onOpenCommandPalette,
	terminalAvailable,
	terminalOpen,
	onToggleTerminal,
}: WorkspacePanelProps) {
	const pathCaption =
		workspacePath && workspacePath.length > 0
			? workspacePath.length > 52
				? `…${workspacePath.slice(-51)}`
				: workspacePath
			: null;
	const hasLoaded = Boolean(sessionSnapshot || sessionEvents.length > 0);
	const hasEmptyThread = !sessionSnapshot && sessionEvents.length === 0;
	const isGitRepo = Boolean(workspaceBranch) || Boolean(workspacePath);

	return (
		<div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-background">
			<header
				className={[
					"border-b border-border pb-2 pt-2 sm:pb-3 sm:pt-3",
					"pl-[calc(env(safe-area-inset-left)+0.75rem)] pr-[calc(env(safe-area-inset-right)+0.75rem)]",
					"sm:pl-[calc(env(safe-area-inset-left)+1.25rem)] sm:pr-[calc(env(safe-area-inset-right)+1.25rem)]",
				].join(" ")}
			>
				<DccWorkbenchChatHeader
					threadTitle={workspaceName}
					projectBadgeLabel={workspaceBranch || null}
					isGitRepo={isGitRepo}
					pathCaption={pathCaption}
					sessionSnapshot={sessionSnapshot}
					terminalAvailable={terminalAvailable}
					terminalOpen={terminalOpen}
					onToggleTerminal={onToggleTerminal}
					onOpenCommandPalette={onOpenCommandPalette}
					onResumeSession={onResumeSession}
					onAbortSession={onAbortSession}
				/>
			</header>

			<div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
				<ActiveThreadViewport
					events={sessionEvents}
					hasLoaded={hasLoaded}
					isEmpty={hasEmptyThread}
				/>

				<div className="border-t border-border/60 px-3 pb-3 pt-3 sm:px-4">
					<WorkspaceComposer
						draftKey={workspaceId}
						disabled={false}
						providerChoices={providerChoices}
						selectedProviderId={selectedProviderId}
						selectedProviderLabel={selectedProviderLabel}
						sessionSnapshot={sessionSnapshot}
						onSelectProvider={onSelectProvider}
						onStartSession={onStartSession}
						onSubmitPrompt={onSubmitPrompt}
						onResumeSession={onResumeSession}
						onAbortSession={onAbortSession}
					/>
				</div>
			</div>
		</div>
	);
}
