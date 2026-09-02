import { Bug, Copy, GitFork, Pencil, RotateCcw } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { TurnEvidenceSummary } from "@dcc/contracts";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { MessageTimestamp } from "./message-metadata";

/** Explains what explicit evidence travelled with this turn, from metadata only. */
function UserMessageEvidenceChip({ evidence }: { evidence: TurnEvidenceSummary }) {
	const { t } = useTranslation("common");
	const detail = evidence.items
		.map((item, index) =>
			[
				`${index + 1}. ${t(`composer.evidence.sources.${item.source}`)}`,
				t(`composer.evidence.trust.${item.trust}`),
				`${item.chars} ${t("conversation.evidence.chars")}`,
				item.truncated ? t("composer.evidence.truncated") : null,
			]
				.filter(Boolean)
				.join(" · "),
		)
		.join("\n");
	return (
		<span
			className="inline-flex items-center gap-1 rounded-full border border-border/60 px-1.5 py-px text-[10px] leading-4 text-muted-foreground"
			title={detail}
			data-testid="user-message-evidence"
		>
			<Bug className="size-3" aria-hidden />
			{t("conversation.evidence.chip", {
				count: evidence.items.length,
				stage: t(`composer.evidence.stages.${evidence.stage}`),
			})}
		</span>
	);
}

export function UserMessage({
	content,
	label,
	createdAt,
	evidence = null,
	retryOfTurnId = null,
	onEdit,
	onFork,
}: {
	content: string;
	label: string;
	createdAt?: string;
	evidence?: TurnEvidenceSummary | null;
	/** Set when this turn explicitly re-ran an aborted turn. */
	retryOfTurnId?: string | null;
	onEdit?: () => void;
	/** Starts a new thread re-anchored on everything before this message. */
	onFork?: () => void;
}) {
	const { t } = useTranslation("common");
	return (
		<div
			data-message-role="user"
			className="conversation-thread-enter conversation-fade-in group/user flex min-w-0 justify-end"
		>
			<div className="relative flex max-w-[75%] min-w-0 flex-col items-end pb-5">
				<div className="conversation-body-text w-full overflow-hidden rounded-md bg-accent/55 px-3 py-2 leading-7">
					<p className="whitespace-pre-wrap break-words text-[13px] text-foreground">
						{content}
					</p>
				</div>
				<div className="mt-1 flex items-center gap-1.5 text-[11px] leading-none text-muted-foreground/60">
					{retryOfTurnId ? (
						<span
							className="inline-flex items-center gap-1 rounded-full border border-border/60 px-1.5 py-px text-[10px] leading-4 text-muted-foreground"
							title={t("conversation.message.retryChipHint")}
							data-testid="user-message-retry"
						>
							<RotateCcw className="size-3" aria-hidden />
							{t("conversation.message.retryChip")}
						</span>
					) : null}
					{evidence && evidence.items.length > 0 ? (
						<UserMessageEvidenceChip evidence={evidence} />
					) : null}
					<MessageTimestamp createdAt={createdAt} />
				</div>
				<div className="pointer-events-none absolute right-1 bottom-0 flex items-center justify-end opacity-0 transition-opacity group-hover/user:pointer-events-auto group-hover/user:opacity-100 group-focus-within/user:pointer-events-auto group-focus-within/user:opacity-100">
					{onFork ? (
						<Button
							type="button"
							variant="ghost"
							size="icon-xs"
							aria-label={t("conversation.message.forkUser")}
							title={t("conversation.message.forkUser")}
							className={cn(
								"pointer-events-auto size-5 shrink-0 text-muted-foreground/28 hover:text-muted-foreground",
								"bg-transparent hover:bg-transparent",
							)}
							onClick={onFork}
						>
							<GitFork className="size-3.5" aria-hidden />
						</Button>
					) : null}
					{onEdit ? (
						<Button
							type="button"
							variant="ghost"
							size="icon-xs"
							aria-label={t("conversation.message.editUser")}
							className={cn(
								"pointer-events-auto size-5 shrink-0 text-muted-foreground/28 hover:text-muted-foreground",
								"bg-transparent hover:bg-transparent",
							)}
							onClick={onEdit}
						>
							<Pencil className="size-3.5" aria-hidden />
						</Button>
					) : null}
					<Button
						type="button"
						variant="ghost"
						size="icon-xs"
						aria-label={t("conversation.message.copyUser", { label })}
						className={cn(
							"pointer-events-auto size-5 shrink-0 text-muted-foreground/28 hover:text-muted-foreground",
							"bg-transparent hover:bg-transparent",
						)}
						onClick={async () => {
							try {
								await navigator.clipboard.writeText(content);
							} catch {
								// Clipboard availability varies across desktop shells; ignore gracefully.
							}
						}}
					>
						<Copy className="size-3.5" aria-hidden />
					</Button>
				</div>
			</div>
		</div>
	);
}
