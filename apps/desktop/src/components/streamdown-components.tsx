import * as React from "react";
import { ExternalLink, FileCode2, FileText } from "lucide-react";
import type { Components, ExtraProps } from "streamdown";
import { CodeBlock } from "@/components/ai/code-block";
import { openExternal, openPath } from "@/lib/shell-api";
import { cn } from "@/lib/utils";
import {
	getStreamdownLinkKind,
	isCodeFileReference,
} from "./streamdown-link-presentation";
import {
	parseWorkspaceFileReference,
} from "./workspace-file-reference";
import {
	type WorkspaceFileLinkContextValue,
	useWorkspaceFileLink,
} from "./workspace-file-link-context";

function getCodeLanguage(children: React.ReactNode) {
	if (!React.isValidElement<{ className?: string }>(children)) {
		return undefined;
	}

	const className = String(children.props.className ?? "");
	const match = /language-([a-z0-9-]+)/i.exec(className);
	return match?.[1];
}

function getCodeContent(children: React.ReactNode) {
	if (!React.isValidElement<{ children?: React.ReactNode }>(children)) {
		return "";
	}

	return String(children.props.children ?? "").replace(/\n$/, "");
}

function StreamdownPre({
	children,
	className,
}: React.ComponentProps<"pre"> & ExtraProps) {
	const language = getCodeLanguage(children);
	const code = getCodeContent(children);

	return <CodeBlock code={code} language={language} className={className} />;
}

async function handleLinkClick(
	event: React.MouseEvent<HTMLAnchorElement>,
	href: string | undefined,
	workspaceFileLink: WorkspaceFileLinkContextValue | null,
) {
	if (!href) {
		return;
	}

	event.preventDefault();
	event.stopPropagation();
	const workspaceReference = parseWorkspaceFileReference(
		href,
		workspaceFileLink?.workspaceRoot,
	);
	if (workspaceReference && workspaceFileLink) {
		workspaceFileLink.onOpenFile(workspaceReference);
		return;
	}

	if (href.startsWith("file://")) {
		await openPath(decodeURIComponent(href.replace("file://", "")));
		return;
	}

	if (/^([a-zA-Z]:\\|\/)/.test(href)) {
		await openPath(href);
		return;
	}

	await openExternal(href);
}

function StreamdownAnchor({
	children,
	className,
	href,
	title,
	...props
}: React.ComponentProps<"a"> & ExtraProps) {
	const workspaceFileLink = useWorkspaceFileLink();
	const workspaceReference = parseWorkspaceFileReference(
		href,
		workspaceFileLink?.workspaceRoot,
	);
	const fileTitle =
		workspaceReference && workspaceFileLink?.getTitle
			? workspaceFileLink.getTitle(workspaceReference)
			: undefined;
	const linkKind = getStreamdownLinkKind(href, workspaceReference);
	const filePath = workspaceReference?.path ?? href ?? "";
	const isFileLink = linkKind === "workspace-file" || linkKind === "file";
	const LinkIcon = isFileLink
		? isCodeFileReference(filePath)
			? FileCode2
			: FileText
		: linkKind === "external"
			? ExternalLink
			: null;
	return (
		<a
			href={href}
			onClick={(event) => {
				void handleLinkClick(event, href, workspaceFileLink);
			}}
			title={fileTitle ?? title}
			className={cn(
				"inline-flex max-w-full items-center gap-1 align-baseline underline-offset-4 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2",
				linkKind === "external" &&
					"text-sky-700 underline decoration-sky-500/70 dark:text-sky-400",
				isFileLink &&
					"cursor-pointer rounded-md border border-primary/20 bg-primary/5 px-1.5 py-0.5 font-medium text-primary no-underline hover:bg-primary/10",
				linkKind === "default" && "text-primary hover:underline",
				className,
			)}
			{...props}
		>
			{LinkIcon ? (
				<LinkIcon className="size-3.5 shrink-0" aria-hidden="true" />
			) : null}
			{children}
		</a>
	);
}

function StreamdownTable({
	children,
	className,
	...props
}: React.ComponentProps<"table"> & ExtraProps) {
	return (
		<div className="my-4 overflow-x-auto rounded-lg border border-border/60">
			<table
				className={cn("w-full border-collapse text-sm", className)}
				{...props}
			>
				{children}
			</table>
		</div>
	);
}

export const streamdownComponents = {
	a: StreamdownAnchor,
	pre: StreamdownPre,
	table: StreamdownTable,
} as Components;
