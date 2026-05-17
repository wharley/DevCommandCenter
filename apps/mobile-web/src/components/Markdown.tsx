import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

/**
 * Minimal markdown renderer tuned for mobile chat bubbles. Tailwind classes are
 * scoped to keep paragraphs tight (no top-margin on the first child, narrow
 * spacing between blocks). No syntax highlighting yet — code blocks render as
 * plain monospace text. Links open in a new tab.
 */
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
						const isBlock = className?.startsWith("language-");
						if (isBlock) {
							return (
								<code className="font-mono text-[12px] text-foreground/90">
									{children}
								</code>
							);
						}
						return (
							<code className="rounded bg-bg/70 px-1 py-0.5 font-mono text-[12px] text-foreground/90">
								{children}
							</code>
						);
					},
					pre: ({ children }) => (
						<pre className="my-1.5 overflow-x-auto rounded-lg bg-bg/80 p-2.5 first:mt-0 last:mb-0">
							{children}
						</pre>
					),
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
