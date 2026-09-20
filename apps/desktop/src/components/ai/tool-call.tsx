import type * as React from "react";
import {
	AlertCircle,
	CheckCircle2,
	ChevronRight,
	CircleDashed,
	FilePenLine,
	Terminal,
} from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { DccThinkingIndicator } from "@/components/DccThinkingIndicator";
import { cn } from "@/lib/utils";

function getDisplayPath(path: string) {
	const segments = path.split("/").filter(Boolean);
	if (segments.length <= 2) {
		return path;
	}
	return `${segments.at(-2)}/${segments.at(-1)}`;
}

export function ToolCall({
	action,
	command,
	file,
	children,
	isLive = false,
	isError = false,
	isUnfinished = false,
}: {
	action: string;
	command?: string;
	file?: string;
	children?: React.ReactNode;
	isLive?: boolean;
	isError?: boolean;
	isUnfinished?: boolean;
}) {
	const { t } = useTranslation("common");
	const [isOpen, setIsOpen] = useState(false);
	const displayFile = useMemo(
		() => (file ? getDisplayPath(file) : null),
		[file],
	);
	const StatusIcon = isError
		? AlertCircle
		: isLive || isUnfinished
			? CircleDashed
			: CheckCircle2;

	return (
		<details
			className="group/tool-call dcc-activity-tool flex min-w-0 flex-col"
			open={isOpen}
		>
			<summary
				onClick={(event) => {
					event.preventDefault();
					setIsOpen((open) => !open);
				}}
				className={cn(
					"-mx-1.5 flex max-w-full cursor-pointer items-center gap-1.5 rounded-md px-1.5 py-1 text-[12px] text-muted-foreground transition-colors hover:bg-muted/35 hover:text-foreground [&::-webkit-details-marker]:hidden",
				)}
			>
				<ChevronRight
					className={cn(
						"size-3 shrink-0 transition-transform",
						isOpen && "rotate-90",
					)}
					aria-hidden
				/>
				<StatusIcon
					className={cn(
						"size-3.5 shrink-0",
						isLive && "text-info",
						isError && "text-destructive",
						!isLive && !isError && !isUnfinished && "text-success",
					)}
					aria-hidden
				/>
				<span className="min-w-0 break-words font-medium text-foreground/85">
					{action}
				</span>
				{isLive ? (
					<span className="ml-auto flex shrink-0 items-center gap-1.5 text-[11px] text-muted-foreground/70">
						{t("conversation.toolCall.running")}
						<DccThinkingIndicator size={12} />
					</span>
				) : isError ? (
					<span className="ml-auto shrink-0 text-[11px] text-destructive">
						{t("conversation.toolCall.failed")}
					</span>
				) : isUnfinished ? (
					<span className="ml-auto text-[11px] text-muted-foreground">
						{t("conversation.activity.timeline.unfinished")}
					</span>
				) : null}
			</summary>
			{isOpen && (
				<div
					tabIndex={0}
					className="mt-1 max-h-64 overflow-auto rounded-md border border-border/45 bg-muted/20 px-3 py-2 text-[12px] leading-6 text-muted-foreground"
				>
					{file ? (
						<div className="mb-2 flex min-w-0 items-center gap-1.5 border-b border-border/40 pb-2 font-mono text-[11px]">
							<FilePenLine className="size-3 shrink-0" aria-hidden />
							<span className="truncate" title={file}>
								{displayFile}
							</span>
						</div>
					) : null}
					{command ? (
						<code className="mb-2 flex min-w-0 items-center gap-1.5 overflow-hidden border-b border-border/40 pb-2 text-[11px]">
							<Terminal className="size-3 shrink-0" aria-hidden />
							<span className="truncate" title={command}>
								{command}
							</span>
						</code>
					) : null}
					{children}
				</div>
			)}
		</details>
	);
}
