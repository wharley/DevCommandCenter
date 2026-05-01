import { cn } from "@/lib/utils";

export function ContextUsageRing({
	value,
	className,
}: {
	value: number;
	className?: string;
}) {
	const clamped = Math.max(0, Math.min(100, value));

	return (
		<div
			className={cn("relative inline-flex size-8 items-center justify-center", className)}
			aria-label={`Context usage ${clamped}%`}
		>
			<svg viewBox="0 0 36 36" className="size-8 -rotate-90">
				<circle
					cx="18"
					cy="18"
					r="15"
					fill="none"
					stroke="currentColor"
					strokeOpacity="0.16"
					strokeWidth="2"
				/>
				<circle
					cx="18"
					cy="18"
					r="15"
					fill="none"
					stroke="currentColor"
					strokeWidth="2"
					strokeDasharray={`${clamped * 0.94} 94`}
					strokeLinecap="round"
				/>
			</svg>
			<span className="absolute text-[10px] font-medium text-muted-foreground">
				{clamped}
			</span>
		</div>
	);
}
