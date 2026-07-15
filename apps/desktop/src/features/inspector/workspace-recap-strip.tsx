import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type {
	WorkspaceRecap,
	WorkspaceRecapAction,
	WorkspaceRecapTone,
} from "./workspace-recap";

const TONE_DOT_CLASS: Record<WorkspaceRecapTone, string> = {
	neutral: "bg-muted-foreground/50",
	working: "bg-emerald-500",
	attention: "bg-amber-500",
	ready: "bg-emerald-500",
	done: "bg-violet-500",
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
				<span
					aria-hidden="true"
					className={cn(
						"size-1.5 shrink-0 rounded-full",
						TONE_DOT_CLASS[recap.tone],
						recap.tone === "working" && "animate-pulse",
					)}
				/>
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
