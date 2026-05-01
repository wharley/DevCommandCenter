import { Suspense } from "react";
import { LazyStreamdown } from "@/components/streamdown-loader";
import { cn } from "@/lib/utils";

function AssistantTextFallback({ text }: { text: string }) {
	return (
		<div className="assistant-markdown-scale max-w-none break-words text-foreground">
			<p className="whitespace-pre-wrap text-[13px] leading-7 text-foreground">
				{text}
			</p>
		</div>
	);
}

export function AssistantMessage({
	content,
	streaming,
}: {
	content: string;
	streaming?: boolean;
}) {
	return (
		<div data-message-role="assistant" className="conversation-fade-in flex min-w-0 justify-start">
			<div className="relative flex min-w-0 max-w-[75%] flex-col pb-5">
				<div className={cn("assistant-markdown-scale max-w-none break-words text-foreground")}>
					<Suspense fallback={<AssistantTextFallback text={content} />}>
						<LazyStreamdown
							mode={streaming ? "streaming" : "static"}
							animated={
								streaming
									? { animation: "blurIn", duration: 150, stagger: 30, sep: "word" }
									: false
							}
							caret={streaming ? "block" : undefined}
							className="conversation-streamdown"
							isAnimating={Boolean(streaming)}
							shikiTheme={["github-light", "github-dark"]}
						>
							{content}
						</LazyStreamdown>
					</Suspense>
				</div>
			</div>
		</div>
	);
}
