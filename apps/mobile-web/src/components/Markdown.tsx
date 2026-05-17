import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { highlightCode } from "@/lib/highlight";

/**
 * Minimal markdown renderer tuned for mobile chat bubbles. Tailwind classes
 * are scoped to keep paragraphs tight (no top-margin on the first child,
 * narrow spacing between blocks). Fenced code blocks are syntax-highlighted
 * with highlight.js (only a curated language set is registered to keep the
 * bundle small). Inline `code` stays unstyled-bold to minimize visual noise.
 * Links open in a new tab.
 */
function HighlightedBlock({
	source,
	language,
}: {
	source: string;
	language: string | null;
}) {
	const { html, language: resolved } = highlightCode(source, language);
	return (
		<pre className="my-1.5 overflow-x-auto rounded-lg bg-bg/80 p-2.5 first:mt-0 last:mb-0">
			<code
				className={resolved ? `hljs language-${resolved}` : "hljs"}
				dangerouslySetInnerHTML={{ __html: html }}
			/>
		</pre>
	);
}

export function Markdown({ text }: { text: string }) {
	return (
		<div className="markdown-body break-words text-[14px] leading-relaxed">
			<ReactMarkdown
				remarkPlugins={[remarkGfm]}
				components={{
					p: ({ children }) => <p className="my-1 first:mt-0 last:mb-0">{children}</p>,
					a: ({ children, href }) => (
						<a
							href={href}
							target="_blank"
							rel="noreferrer noopener"
							className="text-accent underline decoration-accent/40 underline-offset-2"
						>
							{children}
						</a>
					),
					ul: ({ children }) => (
						<ul className="my-1 list-disc space-y-0.5 pl-5 first:mt-0 last:mb-0">{children}</ul>
					),
					ol: ({ children }) => (
						<ol className="my-1 list-decimal space-y-0.5 pl-5 first:mt-0 last:mb-0">{children}</ol>
					),
					li: ({ children }) => <li className="pl-0.5">{children}</li>,
					strong: ({ children }) => <strong className="font-semibold">{children}</strong>,
					em: ({ children }) => <em className="italic">{children}</em>,
					code: ({ children, className }) => {
						const langMatch = /language-([\w-]+)/.exec(className ?? "");
						if (langMatch) {
							const language = langMatch[1] ?? null;
							const source = Array.isArray(children)
								? children.join("")
								: typeof children === "string"
									? children
									: String(children ?? "");
							return <HighlightedBlock source={source.replace(/\n$/, "")} language={language} />;
						}
						return (
							<code className="rounded bg-bg/70 px-1 py-0.5 font-mono text-[12px] text-foreground/90">
								{children}
							</code>
						);
					},
					pre: ({ children }) => <>{children}</>,
					blockquote: ({ children }) => (
						<blockquote className="my-1 border-l-2 border-border pl-3 text-mute first:mt-0 last:mb-0">
							{children}
						</blockquote>
					),
					h1: ({ children }) => (
						<h1 className="mt-2 mb-1 text-[15px] font-semibold first:mt-0 last:mb-0">{children}</h1>
					),
					h2: ({ children }) => (
						<h2 className="mt-2 mb-1 text-[14px] font-semibold first:mt-0 last:mb-0">{children}</h2>
					),
					h3: ({ children }) => (
						<h3 className="mt-2 mb-1 text-[13px] font-semibold first:mt-0 last:mb-0">{children}</h3>
					),
					hr: () => <hr className="my-2 border-border" />,
					table: ({ children }) => (
						<div className="my-1.5 overflow-x-auto first:mt-0 last:mb-0">
							<table className="border-collapse text-[12px]">{children}</table>
						</div>
					),
					th: ({ children }) => (
						<th className="border border-border px-2 py-1 text-left font-medium">{children}</th>
					),
					td: ({ children }) => <td className="border border-border px-2 py-1">{children}</td>,
				}}
			>
				{text}
			</ReactMarkdown>
		</div>
	);
}
