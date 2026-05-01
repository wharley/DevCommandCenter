import type * as React from "react";
import { AlertCircle } from "lucide-react";
import { useState } from "react";
import { DccThinkingIndicator } from "@/components/DccThinkingIndicator";
import { cn } from "@/lib/utils";

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
	const [isOpen, setIsOpen] = useState(true);

	return (
		<details
			className="group/tool-call flex flex-col"
			open={isOpen}
			onToggle={(event) => setIsOpen(event.currentTarget.open)}
		>
			<summary
				className={cn(
					"flex max-w-full items-center gap-1.5 py-0.5 text-[12px] text-muted-foreground [&::-webkit-details-marker]:hidden",
				)}
			>
				<span className="shrink-0 font-medium">{action}</span>
				{file ? (
					<span className="truncate rounded bg-accent/60 px-1.5 py-0.5 font-mono text-[11px] text-muted-foreground">
						{file}
					</span>
				) : null}
				{command ? (
					<code className="inline-block min-w-0 truncate rounded bg-accent/60 px-1.5 py-0.5 font-mono text-[11px] text-muted-foreground">
						{command}
					</code>
				) : null}
				{isLive ? (
					<DccThinkingIndicator size={12} />
				) : isError ? (
					<AlertCircle className="size-3 shrink-0 text-destructive" aria-hidden />
				) : null}
			</summary>
			<div className="pl-4 pt-1 text-[12px] leading-6 text-muted-foreground">
				{children}
			</div>
		</details>
	);
}
