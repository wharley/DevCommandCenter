import { cn } from "@/lib/utils";

export function InlineShortcutDisplay({
	keys,
	className,
}: {
	keys: string[];
	className?: string;
}) {
	return (
		<span
			className={cn(
				"inline-flex items-center gap-px font-medium leading-none tracking-normal text-current",
				className,
			)}
		>
			{keys.map((key, index) => (
				<span
					key={`${key}-${index}`}
					className="inline-flex h-5 min-w-5 items-center justify-center rounded-[4px] border border-border/70 bg-background px-1.5 text-[11px] text-muted-foreground shadow-[inset_0_-1px_0_rgba(0,0,0,0.08)]"
				>
					{key}
				</span>
			))}
		</span>
	);
}
