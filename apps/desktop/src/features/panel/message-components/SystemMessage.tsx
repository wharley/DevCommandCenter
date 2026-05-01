import { MessageTimestamp } from "./message-metadata";

export function SystemMessage({
	label,
	content,
	createdAt,
}: {
	label: string;
	content: string;
	createdAt?: string;
}) {
	return (
		<div
			data-message-role="system"
			className="conversation-thread-enter conversation-fade-in flex min-w-0 justify-center px-4"
		>
			<div className="max-w-[42rem] rounded-md border border-border/60 bg-muted/30 px-3 py-2 text-[12px] leading-6 text-muted-foreground">
				<span className="mr-2 inline-flex rounded bg-background/80 px-1.5 py-0.5 font-mono text-[10px] uppercase tracking-[0.08em] text-muted-foreground">
					{label}
				</span>
				<span className="whitespace-pre-wrap break-words">{content}</span>
				<span className="ml-2 inline-flex items-center gap-1 text-[11px] leading-none text-muted-foreground/60">
					<MessageTimestamp createdAt={createdAt} />
				</span>
			</div>
		</div>
	);
}
