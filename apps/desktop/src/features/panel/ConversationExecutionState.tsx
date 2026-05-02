import { useTranslation } from "react-i18next";
import { DccThinkingIndicator } from "@/components/DccThinkingIndicator";
import { sessionStateLabel } from "@/i18n/session-state-label";

type ConversationExecutionStateProps = {
	workspaceName: string;
	selectedProviderLabel: string | null;
	selectedModelLabel: string | null;
	sessionState: string | null;
	lastTurnState: string | null;
	pendingPrompt: string | null;
};

export function ConversationExecutionState({
	workspaceName,
	selectedProviderLabel,
	selectedModelLabel,
	sessionState,
	lastTurnState,
	pendingPrompt,
}: ConversationExecutionStateProps) {
	const { t } = useTranslation("common");

	return (
		<div className="flex min-h-full flex-1 items-center justify-center px-6 py-10">
			<div className="w-full max-w-2xl rounded-3xl border border-border/60 bg-card/80 p-6 shadow-[0_24px_80px_rgba(0,0,0,0.06)] backdrop-blur-sm sm:p-8">
				<div className="flex items-center gap-3">
					<DccThinkingIndicator size={18} />
					<div className="min-w-0">
						<p className="text-[11px] font-medium uppercase tracking-[0.12em] text-muted-foreground">
							Running session
						</p>
						<h3 className="mt-1 text-[20px] font-semibold tracking-[-0.03em] text-foreground">
							{workspaceName}
						</h3>
					</div>
				</div>

				<p className="mt-4 max-w-xl text-[13px] leading-6 text-muted-foreground">
					The conversation area is intentionally kept visible while the backend
					boots the thread, streams a turn, or catches up with persisted events.
				</p>

				<div className="mt-5 flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
					<span className="rounded-full border border-border/50 bg-background/70 px-2.5 py-1">
						Provider
					</span>
					<span className="rounded-full border border-border/50 bg-background/70 px-2.5 py-1 text-foreground">
						{selectedProviderLabel ?? "Waiting for provider"}
					</span>
					<span className="rounded-full border border-border/50 bg-background/70 px-2.5 py-1">
						Model
					</span>
					<span className="rounded-full border border-border/50 bg-background/70 px-2.5 py-1 text-foreground">
						{selectedModelLabel ?? "Waiting for model"}
					</span>
					{sessionState ? (
						<span className="rounded-full border border-border/50 bg-background/70 px-2.5 py-1 text-foreground">
							{sessionStateLabel(sessionState, t)}
						</span>
					) : null}
					{lastTurnState ? (
						<span className="rounded-full border border-border/50 bg-background/70 px-2.5 py-1 text-foreground">
							{lastTurnState}
						</span>
					) : null}
				</div>

				{pendingPrompt ? (
					<div className="mt-5 rounded-2xl border border-border/60 bg-muted/30 p-4">
						<p className="text-[11px] font-medium uppercase tracking-[0.12em] text-muted-foreground">
							Pending prompt
						</p>
						<p className="mt-2 whitespace-pre-wrap text-[13px] leading-6 text-foreground">
							{pendingPrompt}
						</p>
					</div>
				) : null}
			</div>
		</div>
	);
}
