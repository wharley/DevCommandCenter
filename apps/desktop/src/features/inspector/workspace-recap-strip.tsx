import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type {
	WorkspaceRecap,
	WorkspaceRecapAction,
	WorkspaceRecapTone,
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
		</div>
	);
}
