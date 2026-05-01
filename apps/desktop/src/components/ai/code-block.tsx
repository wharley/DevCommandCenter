import type * as React from "react";
import {
	CodeBlock as StreamdownCodeBlock,
	CodeBlockContainer,
	CodeBlockCopyButton,
	CodeBlockDownloadButton,
	CodeBlockHeader,
} from "streamdown";
import { cn } from "@/lib/utils";

export function CodeBlock({
	code,
	language,
	className,
}: {
	code: string;
	language?: string;
	className?: string;
}) {
	const normalizedLanguage = language ?? "text";

	return (
		<CodeBlockContainer
			language={normalizedLanguage}
			className={cn(
				"group relative my-4 w-full min-w-0 max-w-full overflow-hidden rounded-lg border border-border/70 bg-background/80 shadow-[inset_0_1px_0_rgba(255,255,255,0.02)]",
				className,
			)}
		>
			<div className="flex items-center justify-between border-b border-border/60 px-3 py-2">
				<div className="truncate font-mono text-[10px] uppercase tracking-wide text-muted-foreground/50">
					<CodeBlockHeader language={normalizedLanguage} />
				</div>
				<div className="flex items-center gap-1 opacity-0 transition-opacity group-hover:opacity-100">
					<CodeBlockCopyButton
						code={code}
						className="rounded-md border border-border/50 px-2 py-1 text-[11px] text-muted-foreground hover:bg-accent/60 hover:text-foreground"
					/>
					<CodeBlockDownloadButton
						code={code}
						language={language}
						className="rounded-md border border-border/50 px-2 py-1 text-[11px] text-muted-foreground hover:bg-accent/60 hover:text-foreground"
					/>
				</div>
			</div>
			<StreamdownCodeBlock
				code={code}
				language={normalizedLanguage}
				className="m-0 rounded-none border-0 bg-transparent px-3 py-3 text-[13px]"
			/>
		</CodeBlockContainer>
	);
}
