import {
	CheckCircle2,
	ChevronRight,
	CircleAlert,
	CircleHelp,
	Clock3,
	XCircle,
} from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type {
	WorkspaceRecap,
	WorkspaceRecapAction,
	WorkspaceRecapTone,
	WorkspaceDeliverySignalState,
	WorkspaceDeliveryState,
} from "./workspace-recap";

const TONE_DOT_CLASS: Record<WorkspaceRecapTone, string> = {
	neutral: "bg-muted-foreground/50",
	working: "bg-emerald-500",
	attention: "bg-amber-500",
	ready: "bg-emerald-500",
	done: "bg-violet-500",
};

const STATE_CLASS: Record<WorkspaceDeliveryState, string> = {
	in_development: "border-border/70 bg-muted/50 text-muted-foreground",
	needs_attention:
		"border-amber-500/25 bg-amber-500/10 text-amber-700 dark:text-amber-300",
	blocked:
		"border-destructive/25 bg-destructive/10 text-destructive",
	awaiting_review:
		"border-sky-500/25 bg-sky-500/10 text-sky-700 dark:text-sky-300",
	ready_to_deliver:
		"border-emerald-500/25 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300",
	delivered:
		"border-violet-500/25 bg-violet-500/10 text-violet-700 dark:text-violet-300",
};

const SIGNAL_CLASS: Record<WorkspaceDeliverySignalState, string> = {
	passed: "text-emerald-600 dark:text-emerald-400",
	pending: "text-sky-600 dark:text-sky-400",
	attention: "text-amber-600 dark:text-amber-400",
	blocked: "text-destructive",
	unavailable: "text-muted-foreground",
};

function SignalIcon({ state }: { state: WorkspaceDeliverySignalState }) {
	const className = cn("size-3.5 shrink-0", SIGNAL_CLASS[state]);
	switch (state) {
		case "passed":
			return <CheckCircle2 className={className} />;
		case "pending":
			return <Clock3 className={className} />;
		case "attention":
			return <CircleAlert className={className} />;
		case "blocked":
			return <XCircle className={className} />;
		default:
			return <CircleHelp className={className} />;
	}
}

/**
 * One-line "where is this workspace and what now?" strip pinned below the
 * Inspector mode switch. The presentation layer owns whether the suggested
 * action is shown here or by the authoritative surface below it.
 */
export function WorkspaceRecapStrip({
	recap,
	action,
	requestLabel,
	busy = false,
	onAction,
}: {
	recap: WorkspaceRecap;
	action: WorkspaceRecapAction | null;
	requestLabel: "PR" | "MR";
	busy?: boolean;
	onAction: () => void;
}) {
	const { t } = useTranslation("common");
	const [detailsOpen, setDetailsOpen] = useState(false);
	const message = t(`inspector.recap.messages.${recap.messageKey}`, recap.params);

	return (
		<div
			className="shrink-0 border-t border-border/50 bg-sidebar/85 px-3 py-2"
			role="status"
			aria-label={t("inspector.recap.ariaLabel")}
		>
			<div className="flex items-center gap-2">
				<Badge
					variant="outline"
					className={cn(
						"h-5 shrink-0 gap-1.5 rounded-full px-1.5 text-[9px] font-medium",
						STATE_CLASS[recap.state],
					)}
				>
					<span
						aria-hidden="true"
						className={cn(
							"size-1.5 rounded-full",
							TONE_DOT_CLASS[recap.tone],
							recap.tone === "working" && "animate-pulse",
						)}
					/>
					{t(`inspector.recap.state.${recap.state}`)}
				</Badge>
				<p
					className="min-w-0 flex-1 text-[11.5px] leading-4 text-muted-foreground [display:-webkit-box] [-webkit-box-orient:vertical] [-webkit-line-clamp:2] overflow-hidden"
					title={message}
				>
					{message}
				</p>
				<Button
					type="button"
					variant="ghost"
					size="icon-xs"
					className="size-6 shrink-0 text-muted-foreground"
					onClick={() => setDetailsOpen((value) => !value)}
					aria-expanded={detailsOpen}
					aria-label={t("inspector.recap.signalDetails")}
				>
					<ChevronRight
						className={cn(
							"size-3.5 transition-transform",
							detailsOpen && "rotate-90",
						)}
					/>
				</Button>
				{action ? (
					<Button
						type="button"
						variant="outline"
						size="xs"
						className="h-6 shrink-0 rounded-[7px] px-2 text-[11px] font-medium"
						disabled={busy}
						onClick={onAction}
					>
						{t(action.labelKey, { requestLabel })}
					</Button>
				) : null}
			</div>
			{detailsOpen ? (
				<div className="mt-2 grid gap-1.5 border-t border-border/40 pt-2 sm:grid-cols-2">
					{recap.signals.map((signal) => (
						<div
							key={signal.id}
							className="flex min-w-0 items-start gap-1.5 rounded-md bg-background/45 px-2 py-1.5"
						>
							<SignalIcon state={signal.state} />
							<p className="min-w-0 flex-1 text-[9.5px] leading-4 text-muted-foreground">
								{t(
									`inspector.recap.signals.${signal.messageKey}`,
									signal.params,
								)}
							</p>
							{signal.required ? (
								<span className="shrink-0 text-[8px] font-medium uppercase tracking-[0.06em] text-foreground/55">
									{t("inspector.recap.required")}
								</span>
							) : null}
						</div>
					))}
				</div>
			) : null}
		</div>
	);
}
