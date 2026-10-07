import { memo, Suspense } from "react";
import { LazyStreamdown } from "@/components/streamdown-loader";
import { cn } from "@/lib/utils";
import {
	ASSISTANT_STREAMDOWN_SHIKI_THEME,
	assistantStreamingAnimation,
} from "./assistant-streaming-rendering";
import { usePacedText } from "./use-paced-text";

function AssistantTextFallback({ text }: { text: string }) {
	return (
		<div className="assistant-markdown-scale max-w-none break-words text-foreground">
			<p className="whitespace-pre-wrap text-[13px] leading-7 text-foreground">{text}</p>
		</div>
	);
}

/**
 * Agent prose as Markdown. While streaming, the text is revealed at an even
 * pace so bursts of tokens do not land as jumps.
 */
export const AssistantProse = memo(function AssistantProse({
	content,
	streaming = false,
	className,
}: {
	content: string;
	streaming?: boolean;
	className?: string;
}) {
	const paced = usePacedText(content, streaming);
	return (
		<div className={cn("assistant-markdown-scale max-w-none break-words text-foreground", className)}>
			<Suspense fallback={<AssistantTextFallback text={paced} />}>
				<LazyStreamdown
					mode={streaming ? "streaming" : "static"}
					animated={assistantStreamingAnimation(streaming, paced.length)}
					caret={streaming ? "block" : undefined}
					className="conversation-streamdown"
					isAnimating={streaming}
					shikiTheme={ASSISTANT_STREAMDOWN_SHIKI_THEME}
				>
					{paced}
				</LazyStreamdown>
			</Suspense>
		</div>
	);
});
