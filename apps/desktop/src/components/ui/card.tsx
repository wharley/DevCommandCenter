import type * as React from "react";
import { cn } from "@/lib/utils";

function Card({ className, ...props }: React.ComponentProps<"div">) {
	return (
		<div
			data-slot="card"
			className={cn(
				"dcc-card rounded-[24px] border border-border/80 bg-[linear-gradient(180deg,color-mix(in_oklab,var(--dcc-surface-2)_84%,transparent),color-mix(in_oklab,var(--dcc-surface)_88%,transparent))] p-4 shadow-[0_24px_90px_color-mix(in_oklab,black_14%,transparent)]",
				className,
			)}
			{...props}
		/>
	);
}

function CardHeader({ className, ...props }: React.ComponentProps<"div">) {
	return (
		<div
			data-slot="card-header"
			className={cn("flex items-start justify-between gap-4", className)}
			{...props}
		/>
	);
}

function CardTitle({ className, ...props }: React.ComponentProps<"h3">) {
	return (
		<h3
			data-slot="card-title"
			className={cn("text-lg leading-none font-medium", className)}
			{...props}
		/>
	);
}

function CardDescription({ className, ...props }: React.ComponentProps<"p">) {
	return (
		<p
			data-slot="card-description"
			className={cn("text-sm text-[var(--dcc-text-muted)]", className)}
			{...props}
		/>
	);
}

function CardContent({ className, ...props }: React.ComponentProps<"div">) {
	return <div data-slot="card-content" className={cn("pt-4", className)} {...props} />;
}

function CardFooter({ className, ...props }: React.ComponentProps<"div">) {
	return (
		<div
			data-slot="card-footer"
			className={cn("pt-4", className)}
			{...props}
		/>
	);
}

export { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle };
