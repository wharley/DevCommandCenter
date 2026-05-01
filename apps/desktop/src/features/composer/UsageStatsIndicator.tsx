import { Badge } from "@/components/ui/badge";
import { ContextUsageRing } from "./ContextUsageRing";

export function UsageStatsIndicator({
	turnCount,
	checkpointCount,
	disabled,
}: {
	turnCount: number;
	checkpointCount: number;
	disabled?: boolean;
}) {
	const usage = Math.min(100, turnCount * 12 + checkpointCount * 8);

	return (
		<div className="flex items-center gap-2">
			<ContextUsageRing value={disabled ? 0 : usage} />
			<div className="flex flex-col gap-0.5">
				<Badge variant="outline" className="h-6 px-2 text-[11px] font-normal">
					{turnCount} turns
				</Badge>
				<Badge variant="outline" className="h-6 px-2 text-[11px] font-normal">
					{checkpointCount} checkpoints
				</Badge>
			</div>
		</div>
	);
}
