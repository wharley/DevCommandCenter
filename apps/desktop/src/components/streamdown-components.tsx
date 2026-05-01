import * as React from "react";
import type { Components, ExtraProps } from "streamdown";
import { CodeBlock } from "@/components/ai/code-block";
import { openExternal, openPath } from "@/lib/shell-api";
import { cn } from "@/lib/utils";

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

async function handleLinkClick(event: React.MouseEvent<HTMLAnchorElement>, href?: string) {
	if (!href) {
		return;
	}

	event.preventDefault();
	event.stopPropagation();

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
	...props
}: React.ComponentProps<"a"> & ExtraProps) {
	return (
		<a
			href={href}
			onClick={(event) => {
				void handleLinkClick(event, href);
			}}
			className={cn(
				"text-primary underline-offset-4 hover:underline",
				className,
			)}
			{...props}
		>
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
