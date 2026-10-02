import { memo, useContext, useEffect, useMemo, useState } from "react";
import type { CSSProperties } from "react";
import { ChevronDown, ChevronUp } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
	CodeBlockCopyButton,
	CodeBlockDownloadButton,
	useIsCodeFenceIncomplete,
} from "streamdown";
import { cn } from "@/lib/utils";
import { type HighlightedLines, cachedHighlight, highlightCode } from "./code-highlight";
import {
	CodeBlockPresentationContext,
	formatJsonForDisplay,
	resolveCodeLanguage,
} from "./code-presentation";

/** A block only a few lines over the limit is shown whole: the toggle would cost more than it hides. */
const COLLAPSE_SLACK_LINES = 4;

const ACTION_CLASS =
	"flex size-6 items-center justify-center rounded-md text-muted-foreground/70 hover:bg-accent/70 hover:text-foreground [&_svg]:size-3.5";

function useHighlightedLines(code: string, language: string | null): HighlightedLines | null {
	const [loaded, setLoaded] = useState<{
		code: string;
		language: string;
		lines: HighlightedLines;
	} | null>(null);
	const cached = language ? cachedHighlight(code, language) : null;
	const needsLoad = Boolean(language) && !cached;
	useEffect(() => {
		if (!language || !needsLoad) return;
		let cancelled = false;
		void highlightCode(code, language).then((lines) => {
			if (!cancelled && lines) setLoaded({ code, language, lines });
		});
		return () => {
			cancelled = true;
		};
	}, [code, language, needsLoad]);
	if (cached) return cached;
	return loaded && loaded.code === code && loaded.language === language ? loaded.lines : null;
}

export const CodeBlock = memo(function CodeBlock({
	code,
	language,
	startLine,
	className,
}: {
	code: string;
	language?: string;
	/** File line the snippet starts at. Line numbers show only when this is known. */
	startLine?: number;
	className?: string;
}) {
	const { t } = useTranslation("common");
	const presentation = useContext(CodeBlockPresentationContext);
	// While the fence is still streaming, every chunk is a new string: stay plain.
	const streaming = useIsCodeFenceIncomplete();
	const resolvedLanguage = useMemo(
		() => resolveCodeLanguage(language, code, !streaming),
		[code, language, streaming],
	);
	const displayCode = useMemo(
		() =>
			presentation.formatJson && resolvedLanguage === "json" && !streaming
				? formatJsonForDisplay(code)
				: code,
		[code, presentation.formatJson, resolvedLanguage, streaming],
	);
	const lines = useMemo(() => displayCode.split("\n"), [displayCode]);
	const highlighted = useHighlightedLines(displayCode, streaming ? null : resolvedLanguage);

	const [expanded, setExpanded] = useState(false);
	const collapsedLines = presentation.collapsedLines;
	const collapsible =
		collapsedLines !== undefined && lines.length > collapsedLines + COLLAPSE_SLACK_LINES;
	const visibleCount = collapsible && !expanded ? collapsedLines : lines.length;
	const numbered = startLine !== undefined;

	const actions = (
		<div className="flex items-center gap-0.5 opacity-0 transition-opacity group-hover/code:opacity-100 group-focus-within/code:opacity-100">
			<CodeBlockCopyButton code={displayCode} className={ACTION_CLASS} />
			{presentation.download === false ? null : (
				<CodeBlockDownloadButton
					code={displayCode}
					language={resolvedLanguage ?? undefined}
					className={ACTION_CLASS}
				/>
			)}
		</div>
	);

	return (
		<div
			data-code-block=""
			data-language={resolvedLanguage ?? undefined}
			className={cn(
				"group/code relative my-3 w-full min-w-0 max-w-full overflow-hidden rounded-lg border border-border/60 bg-background",
				className,
			)}
		>
			{resolvedLanguage ? (
				<div className="flex h-8 items-center justify-between border-b border-border/50 pr-1.5 pl-3">
					<span className="truncate font-mono text-[11px] leading-none text-muted-foreground/70">
						{resolvedLanguage}
					</span>
					{actions}
				</div>
			) : (
				<div className="absolute top-1 right-1.5 z-10 rounded-md bg-background/85">{actions}</div>
			)}
			<pre
				className="dcc-code m-0 overflow-x-auto px-3 py-2.5 font-mono text-[12.5px] leading-[1.6] text-foreground"
				data-numbered={numbered || undefined}
				style={
					numbered
						? ({
								counterReset: `dcc-code-line ${startLine - 1}`,
								"--dcc-code-gutter": `${String(startLine + lines.length - 1).length}ch`,
							} as CSSProperties)
						: undefined
				}
			>
				<code>
					{highlighted || numbered
						? lines.slice(0, visibleCount).map((line, index) => (
								<span key={index} className="dcc-code-line">
									{highlighted?.[index]
										? highlighted[index].map((token, tokenIndex) => (
												<span key={tokenIndex} style={token.style}>
													{token.content}
												</span>
											))
										: line}
									{index < visibleCount - 1 ? "\n" : null}
								</span>
							))
						: collapsible && !expanded
							? lines.slice(0, visibleCount).join("\n")
							: displayCode}
				</code>
			</pre>
			{collapsible ? (
				<button
					type="button"
					aria-expanded={expanded}
					className="flex h-7 w-full items-center justify-center gap-1 border-t border-border/50 text-[11px] leading-none text-muted-foreground hover:bg-accent/50 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset"
					onClick={() => setExpanded((value) => !value)}
				>
					{expanded ? (
						<ChevronUp className="size-3" aria-hidden />
					) : (
						<ChevronDown className="size-3" aria-hidden />
					)}
					{expanded
						? t("conversation.message.codeCollapse")
						: t("conversation.message.codeShowAll", { lines: lines.length })}
				</button>
			) : null}
		</div>
	);
});
