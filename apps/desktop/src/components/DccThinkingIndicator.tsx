import { cn } from "@/lib/utils";

export function DccThinkingIndicator({
	size = 16,
	className,
}: {
	size?: number;
	className?: string;
}) {
	return (
		<span
			aria-hidden="true"
			data-slot="dcc-thinking-indicator"
			className={cn("inline-flex shrink-0 items-center justify-center", className)}
			style={{ width: size, height: size }}
		>
			<span className="relative flex size-full items-center justify-center">
				<span className="absolute inset-0 rounded-full border border-border/70" />
				<span
					className="absolute size-1.5 rounded-full bg-foreground/80 animate-[pulse_1.2s_infinite_ease-in-out]"
					style={{ animationDelay: "0ms" }}
				/>
				<span
					className="absolute size-1.5 rounded-full bg-foreground/55 animate-[pulse_1.2s_infinite_ease-in-out]"
					style={{ transform: "translateY(-5px)", animationDelay: "180ms" }}
				/>
				<span
					className="absolute size-1.5 rounded-full bg-foreground/40 animate-[pulse_1.2s_infinite_ease-in-out]"
					style={{ transform: "translateY(5px)", animationDelay: "360ms" }}
				/>
			</span>
		</span>
	);
}
