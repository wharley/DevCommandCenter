import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronRight, CornerDownRight } from "lucide-react";
import type { DelegationHandBack } from "@/features/sessions/delegation-hand-back";
import { cn } from "@/lib/utils";
import { MessageTimestamp } from "./message-metadata";

/**
 * The `[DCC]` turn DCC wrote to the parent agent, shown as what it is: a
 * system event. The text itself was written for the agent (English, task id,
 * worktree paths), so it stays one click away instead of filling the thread.
 */
export function DelegationHandBackRow({
	handBack,
	content,
	createdAt,
}: {
	handBack: DelegationHandBack;
	content: string;
	createdAt?: string;
}) {
	const { t } = useTranslation("common");
	const [open, setOpen] = useState(false);
	const bodyId = useId();

	return (
		<div
			data-message-role="system"
			className="conversation-thread-enter conversation-fade-in flex min-w-0 justify-center px-4"
		>
			<div className="w-full max-w-[42rem] text-[11.5px] leading-5 text-muted-foreground">
				<div className="flex min-w-0 items-center gap-1.5">
					<CornerDownRight className="size-3.5 shrink-0 text-muted-foreground/70" aria-hidden />
					<span className="min-w-0 truncate">
						{t(`delegation.handBack.${handBack.outcome}`)}
					</span>
					<span aria-hidden className="opacity-40">
						·
					</span>
					<button
						type="button"
						aria-expanded={open}
						aria-controls={bodyId}
						onClick={() => setOpen((value) => !value)}
						className="inline-flex shrink-0 items-center gap-0.5 rounded-sm text-muted-foreground/80 underline-offset-2 hover:text-foreground hover:underline"
					>
						<ChevronRight
							className={cn("size-3 transition-transform", open && "rotate-90")}
							aria-hidden
						/>
						{t("delegation.handBack.showSent")}
					</button>
					<span className="ml-auto inline-flex shrink-0 items-center text-[11px] leading-none text-muted-foreground/60">
						<MessageTimestamp createdAt={createdAt} />
					</span>
				</div>
				{open ? (
					<pre
						id={bodyId}
						className="mt-1.5 max-h-72 overflow-auto whitespace-pre-wrap break-words rounded-md border border-border/60 bg-muted/30 px-3 py-2 font-mono text-[11px] leading-[1.55] text-muted-foreground"
					>
						{content}
					</pre>
				) : null}
			</div>
		</div>
	);
}
