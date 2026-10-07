import {
	AlertCircle,
	Bot,
	Check,
	CheckCircle2,
	ChevronRight,
	CircleDashed,
	Copy,
	FileText,
	FilePenLine,
	Globe,
	ListTodo,
	Plug,
	Search,
	Terminal,
	Wrench,
	type LucideIcon,
} from "lucide-react";
import { memo, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { DccThinkingIndicator } from "@/components/DccThinkingIndicator";
import type { WorkspaceMessageAnnotation } from "@/features/sessions/session-thread-history.logic";
import {
	classifyDiffLine,
	classifyToolAction,
	diffStats,
	mcpToolLabel,
	stripAnsi,
	toolTarget,
	type ToolKind,
} from "@/features/panel/message-components/turn-activity.logic";
import { cn } from "@/lib/utils";

export type ToolCallAnnotation = Extract<WorkspaceMessageAnnotation, { type: "tool-call" }>;

const KIND_ICON: Record<ToolKind, LucideIcon> = {
	command: Terminal,
	edit: FilePenLine,
	read: FileText,
	search: Search,
	web: Globe,
	plan: ListTodo,
	agent: Bot,
	mcp: Plug,
	other: Wrench,
};

function CopyButton({ value, label }: { value: string; label: string }) {
	const [copied, setCopied] = useState(false);
	return (
		<button
			type="button"
			className="dcc-tool-copy"
			aria-label={label}
			title={label}
			onClick={async (event) => {
				event.stopPropagation();
				try {
					await navigator.clipboard.writeText(value);
					setCopied(true);
					window.setTimeout(() => setCopied(false), 1200);
				} catch {
					// Clipboard availability varies across desktop shells.
				}
			}}
		>
			{copied ? <Check className="size-3" aria-hidden /> : <Copy className="size-3" aria-hidden />}
		</button>
	);
}

export function ToolDiffBlock({ diff }: { diff: string }) {
	const lines = useMemo(() => diff.replace(/\n$/, "").split("\n"), [diff]);
	return (
		<pre className="dcc-tool-diff" tabIndex={0}>
			{lines.map((line, index) => (
				<span
					// Diff lines have no identity beyond their position.
					key={index}
					className="dcc-tool-diff-line"
					data-kind={classifyDiffLine(line)}
				>
					{line || " "}
				</span>
			))}
		</pre>
	);
}

function ToolCallDetails({
	annotation,
	failed,
}: {
	annotation: ToolCallAnnotation;
	failed: boolean;
}) {
	const { t } = useTranslation("common");
	const detail = annotation.detail;
	const command = detail?.command ?? annotation.command;
	const file = detail?.file ?? annotation.file;
	const output = useMemo(() => {
		const raw = detail?.output ?? (annotation.content.trim() ? annotation.content : "");
		return raw ? stripAnsi(raw).replace(/\s+$/, "") : "";
	}, [annotation.content, detail?.output]);
	const kind = classifyToolAction(annotation.action);
	const showCommand = Boolean(command) && kind !== "edit";
	return (
		<div className="dcc-tool-details">
			{file && kind !== "command" ? (
				<div className="dcc-tool-file" title={file}>
					<FileText className="size-3 shrink-0" aria-hidden />
					<span className="truncate">{file}</span>
				</div>
			) : null}
			{showCommand ? (
				<div className="dcc-tool-section">
					<div className="dcc-tool-section-head">
						<span>{t("conversation.toolCall.command")}</span>
						<CopyButton value={command ?? ""} label={t("conversation.toolCall.copy")} />
					</div>
					<pre className="dcc-tool-command">
						<span aria-hidden>$ </span>
						{command}
					</pre>
				</div>
			) : null}
			{detail?.diff ? (
				<div className="dcc-tool-section">
					<div className="dcc-tool-section-head">
						<span>{t("conversation.toolCall.changes")}</span>
						<CopyButton value={detail.diff} label={t("conversation.toolCall.copy")} />
					</div>
					<ToolDiffBlock diff={detail.diff} />
				</div>
			) : null}
			{output ? (
				<div className="dcc-tool-section">
					<div className="dcc-tool-section-head">
						<span>{t("conversation.toolCall.output")}</span>
						{typeof detail?.exitCode === "number" ? (
							<span className="dcc-tool-exit" data-failed={detail.exitCode !== 0 ? "true" : "false"}>
								{t("conversation.toolCall.exitCode", { code: detail.exitCode })}
							</span>
						) : null}
						<CopyButton value={output} label={t("conversation.toolCall.copy")} />
					</div>
					<pre className="dcc-tool-output" data-failed={failed ? "true" : "false"} tabIndex={0}>
						{output}
					</pre>
				</div>
			) : failed ? (
				<p className="dcc-tool-failure">
					{annotation.status?.reason ?? t("conversation.toolCall.failedFallback")}
				</p>
			) : !detail?.diff && !detail?.input ? (
				<p className="dcc-tool-empty">{t("conversation.toolCall.noOutput")}</p>
			) : null}
			{detail?.truncated ? (
				<p className="dcc-tool-empty">{t("conversation.toolCall.truncated")}</p>
			) : null}
			{detail?.input && !detail.diff ? (
				<details className="dcc-tool-input">
					<summary>{t("conversation.toolCall.input")}</summary>
					<pre>{detail.input}</pre>
				</details>
			) : null}
		</div>
	);
}

/**
 * One tool call: what it did on a single line (verb, target, result badges),
 * with the command, output, exit code and diff one click away.
 */
export const ToolCallRow = memo(function ToolCallRow({
	annotation,
	live,
	defaultOpen = false,
}: {
	annotation: ToolCallAnnotation;
	/** The turn is still running, so an unfinished call is in progress. */
	live: boolean;
	defaultOpen?: boolean;
}) {
	const { t } = useTranslation("common");
	const [open, setOpen] = useState(defaultOpen);
	const failed = annotation.status?.type === "failed";
	const running = !failed && live && Boolean(annotation.streaming);
	const unfinished = !failed && !live && Boolean(annotation.streaming);
	const kind = classifyToolAction(annotation.action);
	const Icon = KIND_ICON[kind];
	const target =
		toolTarget(annotation) ??
		(kind === "mcp" ? mcpToolLabel(annotation.action) : kind === "other" ? annotation.action : null);
	const stats = diffStats(annotation.detail?.diff);
	const exitCode = annotation.detail?.exitCode;
	const verb = t(`conversation.toolCall.verbs.${kind}.${running ? "live" : "done"}`);
	const StatusIcon = failed ? AlertCircle : running || unfinished ? CircleDashed : CheckCircle2;
	return (
		<div className="dcc-tool" data-open={open ? "true" : "false"} data-failed={failed ? "true" : "false"}>
			<button
				type="button"
				className="dcc-tool-summary"
				aria-expanded={open}
				onClick={() => setOpen((value) => !value)}
			>
				<ChevronRight className="dcc-tool-chevron size-3 shrink-0" aria-hidden />
				<Icon className="dcc-tool-kind size-3.5 shrink-0" aria-hidden />
				<span className="dcc-tool-verb">{verb}</span>
				{target ? (
					<span className="dcc-tool-target" data-mono={kind === "command" ? "true" : "false"} title={target}>
						{target}
					</span>
				) : null}
				<span className="dcc-tool-badges">
					{stats ? (
						<span className="dcc-tool-stats">
							{stats.additions ? <span data-kind="add">+{stats.additions}</span> : null}
							{stats.deletions ? <span data-kind="del">−{stats.deletions}</span> : null}
						</span>
					) : null}
					{typeof exitCode === "number" && exitCode !== 0 ? (
						<span className="dcc-tool-exit" data-failed="true">
							{t("conversation.toolCall.exitCode", { code: exitCode })}
						</span>
					) : null}
					{running ? (
						<DccThinkingIndicator size={12} />
					) : unfinished ? (
						<span className="dcc-tool-note">{t("conversation.activity.timeline.unfinished")}</span>
					) : (
						<StatusIcon
							className={cn("size-3 shrink-0", failed ? "text-destructive" : "dcc-tool-ok")}
							aria-label={failed ? t("conversation.toolCall.failed") : undefined}
						/>
					)}
				</span>
			</button>
			{open ? <ToolCallDetails annotation={annotation} failed={failed} /> : null}
		</div>
	);
});
