import { ArrowRight, Plus, Sparkles } from "lucide-react";
import { Button } from "@/components/ui/button";

const SUGGESTED_PROMPTS = [
	"Summarize what this workspace already contains.",
	"Find the main entry points in this repo.",
	"Show me the next best action to continue from zero.",
];

type ConversationLaunchStateProps = {
	workspaceName: string;
	selectedProviderLabel: string | null;
	selectedModelLabel: string | null;
	onStartSession: () => void;
	onSubmitPrompt: (prompt: string) => Promise<void>;
};

export function ConversationLaunchState({
	workspaceName,
	selectedProviderLabel,
	selectedModelLabel,
	onStartSession,
	onSubmitPrompt,
}: ConversationLaunchStateProps) {
	return (
		<div className="flex min-h-full flex-1 items-center justify-center px-6 py-10">
			<div className="w-full max-w-2xl rounded-3xl border border-border/60 bg-card/80 p-6 shadow-[0_24px_80px_rgba(0,0,0,0.06)] backdrop-blur-sm sm:p-8">
				<div className="flex items-start justify-between gap-4">
					<div className="min-w-0">
						<div className="mb-2 inline-flex items-center gap-2 rounded-full border border-border/60 bg-muted/40 px-3 py-1 text-[11px] font-medium uppercase tracking-[0.12em] text-muted-foreground">
							<Sparkles className="size-3.5" aria-hidden />
							<span>New conversation</span>
						</div>
						<h3 className="text-[20px] font-semibold tracking-[-0.03em] text-foreground">
							{workspaceName}
						</h3>
						<p className="mt-2 max-w-xl text-[13px] leading-6 text-muted-foreground">
							Start from scratch with a blank thread. Use one of the prompts below
							or launch the session first and build from there.
						</p>
					</div>
					<Button
						type="button"
						variant="outline"
						size="sm"
						className="shrink-0 rounded-full"
						onClick={onStartSession}
					>
						<Plus className="size-3.5" aria-hidden />
						Start session
					</Button>
				</div>

				<div className="mt-5 flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
					<span className="rounded-full border border-border/50 bg-background/70 px-2.5 py-1">
						Provider
					</span>
					<span className="rounded-full border border-border/50 bg-background/70 px-2.5 py-1 text-foreground">
						{selectedProviderLabel ?? "Select a provider below"}
					</span>
					<span className="rounded-full border border-border/50 bg-background/70 px-2.5 py-1">
						Model
					</span>
					<span className="rounded-full border border-border/50 bg-background/70 px-2.5 py-1 text-foreground">
						{selectedModelLabel ?? "Select a model"}
					</span>
				</div>

				<div className="mt-5 grid gap-2">
					{SUGGESTED_PROMPTS.map((prompt) => (
						<Button
							key={prompt}
							type="button"
							variant="outline"
							className="h-auto justify-start rounded-2xl border-border/60 px-4 py-3 text-left"
							onClick={() => {
								void onSubmitPrompt(prompt);
							}}
						>
							<div className="flex min-w-0 items-center gap-2">
								<ArrowRight className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
								<span className="min-w-0 text-[13px] leading-5 text-foreground">
									{prompt}
								</span>
							</div>
						</Button>
					))}
				</div>

				<p className="mt-4 text-[11px] leading-6 text-muted-foreground">
					The conversation will appear here once you send the first prompt.
				</p>
			</div>
		</div>
	);
}
