import type { ReactNode } from "react";
import { cn } from "@/lib/cn";

/**
 * Shared mission-control primitives. The visual language: monospace for
 * identity/labels (the "instrument" voice), a state-coded node per agent,
 * and quiet panels so the one live thing on screen is the loud thing.
 */

export type AgentState = "live" | "wait" | "fail" | "idle";

const STATE_COLOR: Record<AgentState, string> = {
	live: "var(--color-accent)",
	wait: "var(--color-wait)",
	fail: "var(--color-danger)",
	idle: "var(--color-faint)",
};

/** A status node. Filled + breathing when live; hollow when idle. */
export function StateDot({
	state,
	className,
}: {
	state: AgentState;
	className?: string;
}) {
	const color = STATE_COLOR[state];
	const hollow = state === "idle";
	return (
		<span
			className={cn(
				"inline-block size-2 shrink-0 rounded-full",
				state === "live" && "dcc-live",
				className,
			)}
			style={{
				background: hollow ? "transparent" : color,
				border: hollow ? `1.5px solid ${color}` : undefined,
			}}
			aria-hidden
		/>
	);
}

/** Section eyebrow: mono label + optional count, the structural rhythm of
 *  every screen. The count earns its place only when there's something to
 *  count. */
export function SectionLabel({
	children,
	count,
	tone = "mute",
}: {
	children: ReactNode;
	count?: number;
	tone?: "mute" | "wait" | "live";
}) {
	const toneClass =
		tone === "wait" ? "text-wait" : tone === "live" ? "text-accent" : "text-mute";
	return (
		<div className="mb-2.5 flex items-center gap-2 px-0.5">
			<span
				className={cn(
					"font-mono text-[10px] font-semibold uppercase tracking-[0.18em]",
					toneClass,
				)}
			>
				{children}
			</span>
			{count !== undefined && count > 0 ? (
				<span
					className={cn(
						"grid min-w-[17px] place-items-center rounded-full px-1 font-mono text-[10px] font-bold tabular-nums",
						tone === "wait"
							? "bg-wait text-[var(--color-wait-ink)]"
							: tone === "live"
								? "bg-accent text-[var(--color-accent-ink)]"
								: "bg-elevated text-mute",
					)}
				>
					{count}
				</span>
			) : null}
			<span className="h-px flex-1 bg-border/70" />
		</div>
	);
}

/** The page frame. One source of truth for max width + safe padding. */
export function Shell({
	children,
	className,
}: {
	children: ReactNode;
	className?: string;
}) {
	return (
		<main
			className={cn(
				"mx-auto flex min-h-dvh max-w-md flex-col px-5 py-7",
				className,
			)}
		>
			{children}
		</main>
	);
}

/** Empty / loading rests — direction, never just mood. */
export function Rest({
	icon,
	title,
	children,
}: {
	icon?: ReactNode;
	title?: string;
	children?: ReactNode;
}) {
	return (
		<div className="flex flex-col items-center gap-2 rounded-2xl border border-dashed border-border/70 px-6 py-9 text-center">
			{icon ? <div className="text-faint">{icon}</div> : null}
			{title ? <p className="text-[13px] font-medium text-foreground">{title}</p> : null}
			{children ? (
				<p className="text-[12px] leading-relaxed text-mute">{children}</p>
			) : null}
		</div>
	);
}

/** The DCC wordmark — mono, tracked, with a phosphor connection dot. */
export function Wordmark({ online }: { online?: boolean }) {
	return (
		<div className="flex items-center gap-2">
			<span className="font-mono text-[15px] font-bold tracking-tight text-foreground">
				DCC
			</span>
			<span className="font-mono text-[10px] uppercase tracking-[0.22em] text-faint">
				mission&nbsp;control
			</span>
			{online !== undefined ? (
				<StateDot state={online ? "live" : "idle"} className="ml-0.5" />
			) : null}
		</div>
	);
}

