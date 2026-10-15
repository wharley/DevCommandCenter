import { Activity, X } from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { StageChatTab, type StageChat } from "@/features/editor/stage-chat-tab";
import { cn } from "@/lib/utils";

/** The session's activity, context and spec as a tab on the stage. */
export function SessionStageSurface({
	stage,
	content,
	onClose,
}: {
	stage: StageChat;
	content: ReactNode;
	onClose: () => void;
}) {
	const { t } = useTranslation("common");
	return (
		<section className="flex h-full min-h-0 min-w-0 flex-col overflow-hidden bg-background text-foreground">
			<div className="flex h-9 shrink-0 items-center border-b border-border">
				<div
					role="tablist"
					className="flex min-w-0 flex-1 items-center gap-0.5 overflow-x-auto px-2"
				>
					<StageChatTab stage={stage} />
					<div
						role="tab"
						aria-selected={!stage.chatActive}
						onClick={stage.onShowContent}
						className={cn(
							"group flex h-7 shrink-0 cursor-pointer items-center gap-1.5 rounded-md px-2 text-[11px] transition-colors",
							!stage.chatActive
								? "bg-muted/60 text-foreground shadow-sm"
								: "text-muted-foreground hover:bg-muted/30 hover:text-foreground",
						)}
					>
						<Activity className="size-3.5 shrink-0" strokeWidth={1.8} aria-hidden />
						<span>{t("sessionStage.title")}</span>
						<button
							type="button"
							aria-label={t("fileTabs.close")}
							onClick={(event) => {
								event.stopPropagation();
								onClose();
							}}
							className="shrink-0 rounded p-0.5 text-muted-foreground opacity-0 hover:text-foreground group-hover:opacity-100"
						>
							<X className="size-3" strokeWidth={2} />
						</button>
					</div>
				</div>
			</div>
			<div
				className={cn(
					"flex min-h-0 flex-1 flex-col overflow-hidden",
					stage.chatActive && "invisible",
				)}
			>
				{content}
			</div>
		</section>
	);
}
