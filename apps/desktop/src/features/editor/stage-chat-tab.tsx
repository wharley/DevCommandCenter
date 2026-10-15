import { Loader2, MessageSquare } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";

/**
 * Content on the workbench stage (open files, a diff) sits next to a Chat tab.
 * While the Chat tab is active only the tab strip shows.
 */
export type StageChat = {
	chatActive: boolean;
	/** What the agent did while the content was in front. */
	chatStatus: "idle" | "working" | "unread";
	onShowChat: () => void;
	onShowContent: () => void;
};

export function StageChatTab({ stage }: { stage: StageChat }) {
	const { t } = useTranslation("common");
	const label = t("fileTabs.chat");
	const statusLabel =
		stage.chatStatus === "working"
			? t("fileTabs.chatWorking")
			: stage.chatStatus === "unread"
				? t("fileTabs.chatUnread")
				: null;
	return (
		<button
			type="button"
			role="tab"
			aria-selected={stage.chatActive}
			onClick={stage.onShowChat}
			title={statusLabel ?? label}
			className={cn(
				"flex h-7 shrink-0 cursor-pointer items-center gap-1.5 rounded-md px-2.5 transition-colors",
				stage.chatActive
					? "bg-muted/60 text-foreground shadow-sm"
					: "text-muted-foreground hover:bg-muted/30 hover:text-foreground",
			)}
		>
			<MessageSquare className="size-3.5 shrink-0" strokeWidth={1.8} aria-hidden />
			<span className="text-[11px]">{label}</span>
			{stage.chatStatus === "working" ? (
				<Loader2
					className="size-3 shrink-0 animate-spin text-sky-500"
					strokeWidth={2}
					aria-label={statusLabel ?? undefined}
				/>
			) : stage.chatStatus === "unread" ? (
				<span
					className="size-1.5 shrink-0 rounded-full bg-sky-500"
					aria-label={statusLabel ?? undefined}
				/>
			) : null}
		</button>
	);
}
