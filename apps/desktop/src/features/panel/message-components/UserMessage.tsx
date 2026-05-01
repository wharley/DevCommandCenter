import { Copy } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export function UserMessage({
	content,
	label,
}: {
	content: string;
	label: string;
}) {
	return (
		<div data-message-role="user" className="conversation-fade-in group/user flex min-w-0 justify-end">
			<div className="relative flex max-w-[75%] min-w-0 flex-col items-end pb-5">
				<div className="conversation-body-text w-full overflow-hidden rounded-md bg-accent/55 px-3 py-2 leading-7">
					<p className="whitespace-pre-wrap break-words text-[13px] text-foreground">
						{content}
					</p>
				</div>
				<div className="pointer-events-none absolute right-1 bottom-0 flex items-center justify-end opacity-0 transition-opacity group-hover/user:pointer-events-auto group-hover/user:opacity-100 group-focus-within/user:pointer-events-auto group-focus-within/user:opacity-100">
					<Button
						type="button"
						variant="ghost"
						size="icon-xs"
						aria-label={`Copy ${label.toLowerCase()} message`}
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
