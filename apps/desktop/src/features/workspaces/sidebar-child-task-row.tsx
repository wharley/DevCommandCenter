import { memo } from "react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import type { ChildTaskEntry } from "./sidebar-child-tasks.logic";

/**
 * A task the agent of this workspace proposed or started (`dcc_propose_task`),
 * nested under the workspace's row the way a coordinator's phases hang under
 * it. Green while it works, grey while it is open but stopped, amber when it
 * waits on the person: a proposal to start, an approval, edits to review.
 */
export const SidebarChildTaskRow = memo(function SidebarChildTaskRow({
	task,
	last = false,
	selected = false,
	onSelect,
}: {
	task: ChildTaskEntry;
	/** Its conversation is the one on screen. */
	selected?: boolean;
	/** The last task of its workspace: the guide line stops at its row. */
	last?: boolean;
	onSelect?: () => void;
}) {
	const { t } = useTranslation("common");
	const waitsOnPerson =
		task.state === "proposed" || task.state === "waiting" || task.state === "review";
	return (
		<button
			type="button"
			data-child-task={task.id}
			data-state={task.state}
			aria-current={selected ? "location" : undefined}
			disabled={!onSelect}
			onClick={onSelect}
			title={task.title}
			className={cn(
				"group/dccChildTask relative flex h-full w-full min-w-0 cursor-pointer items-center gap-2 rounded-md pl-9 pr-3 text-left transition-colors hover:bg-accent/60 hover:text-foreground disabled:cursor-default",
				selected ? "bg-accent text-foreground" : "text-muted-foreground",
			)}
		>
			{/* The guide from the workspace row down through its tasks, with a tick to each. */}
			<span
				aria-hidden
				data-guide={last ? "last" : "through"}
				className={cn("absolute left-[22px] top-0 w-px bg-border", last ? "h-1/2" : "h-full")}
			/>
			<span aria-hidden className="absolute left-[22px] top-1/2 h-px w-2.5 bg-border" />
			<span
				aria-hidden
				className={cn(
					"size-1.5 shrink-0 rounded-full",
					task.state === "working" && "animate-pulse bg-emerald-500",
					task.state === "idle" && "bg-muted-foreground/40",
					(task.state === "review" || task.state === "waiting") && "bg-amber-500",
					task.state === "waiting" && "animate-pulse",
					task.state === "proposed" && "border border-amber-500 bg-transparent",
				)}
			/>
			{/* Sized on spans: a global font reset on button would undo it here. */}
			<span className="min-w-0 flex-1 truncate text-[12px] leading-5">{task.title}</span>
			{waitsOnPerson ? (
				<span className="shrink-0 text-[10.5px] font-medium leading-5 text-amber-700 dark:text-amber-300">
					{t(`sidebar.childTask.${task.state}`)}
				</span>
			) : null}
		</button>
	);
});
