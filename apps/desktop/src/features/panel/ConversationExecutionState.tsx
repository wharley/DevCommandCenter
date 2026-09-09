import { useTranslation } from "react-i18next";
import { Bot, Check, MessageSquarePlus, Send, Sparkles } from "lucide-react";
import { cn } from "@/lib/utils";
import type { ConversationStartingPhase } from "./conversation-starting.logic";

type ConversationExecutionStateProps = {
	phase?: ConversationStartingPhase;
};

/**
 * Keep one stable preparation surface until the first agent activity. The
 * prompt is revealed only in the real timeline, at its final position.
 */
export function ConversationExecutionState({
	phase = "creating",
}: ConversationExecutionStateProps) {
	return (
		<div className="flex min-h-full flex-1 flex-col px-5 py-6">
			<ConversationStartingIndicator phase={phase} />
		</div>
	);
}

export function ConversationStartingIndicator({
	phase = "waiting",
}: { phase?: ConversationStartingPhase }) {
	const { t } = useTranslation("common");
	const steps = [
		{ id: "creating", icon: MessageSquarePlus },
		{ id: "sending", icon: Send },
		{ id: "waiting", icon: Bot },
	] as const;
	const activeIndex = steps.findIndex(step => step.id === phase);

	return (
		<div role="status" aria-live="polite" aria-atomic="true" className="conversation-thread-enter conversation-fade-in flex min-w-0 justify-start motion-reduce:animate-none">
			<div className="w-full max-w-[20rem] rounded-xl border border-border/60 bg-muted/15 px-3.5 py-3 shadow-[0_1px_3px_rgb(0_0_0/0.025)]">
				<div className="mb-2 flex items-center gap-2 text-[12px] font-medium text-foreground/90">
					<Sparkles className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
					<span>{t(`conversation.startup.${phase}.title`)}</span>
				</div>
				<ol className="space-y-0.5">
					{steps.map((step, index) => {
						const completed = index < activeIndex;
						const active = index === activeIndex;
						const Icon = completed ? Check : step.icon;
						return (
							<li key={step.id} aria-current={active ? "step" : undefined} data-startup-state={completed ? "complete" : active ? "active" : "pending"} className="relative flex min-h-8 items-center gap-2.5">
								{index < steps.length - 1 ? (
									<span aria-hidden className={cn("absolute left-[10px] top-[26px] h-3 w-px transition-colors duration-300", completed ? "bg-foreground/25" : "bg-border/70")} />
								) : null}
								<span aria-hidden className={cn("relative flex size-[21px] shrink-0 items-center justify-center rounded-full transition-colors duration-300", active ? "bg-background text-foreground shadow-[0_0_10px_rgb(128_128_128/0.10)]" : completed ? "text-muted-foreground" : "text-muted-foreground/35")}>
									{active ? <span className="absolute inset-0 rounded-full border border-foreground/10 border-t-foreground/65 motion-safe:animate-[spin_1.8s_linear_infinite]" /> : null}
									<Icon className={active ? "size-[11px]" : "size-3"} strokeWidth={completed ? 2.2 : 1.7} />
								</span>
								<span className={cn("text-[12px] leading-5 transition-colors duration-300", active ? "font-medium text-foreground/90" : completed ? "text-muted-foreground" : "text-muted-foreground/45")}>
									{t(`conversation.startup.${step.id}.step`)}
								</span>
							</li>
						);
					})}
				</ol>
				<p className="mt-2 border-t border-border/40 pt-2 text-[11px] leading-[17px] text-muted-foreground">{t(`conversation.startup.${phase}.description`)}</p>
			</div>
		</div>
	);
}
