import type * as React from "react";
import {
	AlertCircle,
	CheckCircle2,
	ChevronRight,
	CircleDashed,
	FilePenLine,
	Terminal,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
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
}: {
	action: string;
	command?: string;
	file?: string;
	children?: React.ReactNode;
	isLive?: boolean;
	isError?: boolean;
}) {
	const { t } = useTranslation("common");
	const shouldStayOpen = isLive || isError;
	const [isOpen, setIsOpen] = useState(shouldStayOpen);
	// Once the user toggles by hand, auto open/close stops driving this disclosure.
	const userToggledRef = useRef(false);
	const displayFile = useMemo(() => (file ? getDisplayPath(file) : null), [file]);
	const StatusIcon = isLive ? CircleDashed : isError ? AlertCircle : CheckCircle2;

	useEffect(() => {
		if (userToggledRef.current) {
			return;
		}

		setIsOpen(shouldStayOpen);
	}, [shouldStayOpen]);

	return (
		<details className="group/tool-call flex min-w-0 flex-col" open={isOpen}>
			<summary
				onClick={(event) => {
					event.preventDefault();
					userToggledRef.current = true;
					setIsOpen((open) => !open);
				}}
				className={cn(
					"-mx-1.5 flex max-w-full cursor-pointer items-center gap-1.5 rounded-md px-1.5 py-1 text-[12px] text-muted-foreground transition-colors hover:bg-muted/35 hover:text-foreground [&::-webkit-details-marker]:hidden",
				)}
			>
				<ChevronRight
					className={cn("size-3 shrink-0 transition-transform", isOpen && "rotate-90")}
					aria-hidden
				/>
				<StatusIcon
					className={cn(
						"size-3.5 shrink-0",
						isLive && "text-info",
						isError && "text-destructive",
						!isLive && !isError && "text-success",
					)}
					aria-hidden
				/>
				<span className="shrink-0 font-medium text-foreground/85">{action}</span>
				{file ? (
					<span
						title={file}
						className="inline-flex min-w-0 items-center gap-1 truncate rounded bg-accent/60 px-1.5 py-0.5 font-mono text-[11px] text-muted-foreground"
					>
						<FilePenLine className="size-3 shrink-0" aria-hidden />
						<span className="truncate">{displayFile}</span>
					</span>
				) : null}
				{command ? (
					<code
						title={command}
						className="inline-flex min-w-0 items-center gap-1 truncate rounded bg-accent/60 px-1.5 py-0.5 font-mono text-[11px] text-muted-foreground"
					>
						<Terminal className="size-3 shrink-0" aria-hidden />
						<span className="truncate">{command}</span>
					</code>
				) : null}
				{isLive ? (
					<span className="ml-auto flex shrink-0 items-center gap-1.5 text-[11px] text-muted-foreground/70">
						{t("conversation.toolCall.running")}
						<DccThinkingIndicator size={12} />
					</span>
				) : isError ? (
					<span className="ml-auto shrink-0 text-[11px] text-destructive">
						{t("conversation.toolCall.failed")}
					</span>
				) : null}
			</summary>
			<div className="mt-1 max-h-64 overflow-auto rounded-md border border-border/45 bg-muted/20 px-3 py-2 text-[12px] leading-6 text-muted-foreground">
				{children}
			</div>
		</details>
	);
}
