import { cn } from "@/lib/utils";

/**
 * DCC session counters from the Rust snapshot.
 * UsageStatsIndicator — compact row so the footer matches the reference layout.
 */
export function UsageStatsIndicator({
	turnCount,
	checkpointCount,
	disabled,
	className,
}: {
	turnCount: number;
	checkpointCount: number;
	disabled?: boolean;
	className?: string;
}) {
	if (disabled) {
		return null;
	}

	return (
		<div
			className={cn(
				"hidden items-center gap-1.5 text-[11px] tabular-nums text-muted-foreground sm:flex",
				className,
			)}
		>
			<span>{turnCount} turns</span>
			<span className="text-border/80">·</span>
			<span>{checkpointCount} checkpoints</span>
		</div>
	);
}
