import { X } from "lucide-react";
import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

/** Blueprint §7.5 — inline chip for Lexical decorator badges inside the composer. */
export function ComposerInlineBadge({
	icon,
	label,
	onRemove,
	className,
}: {
	icon: ReactNode;
	label: string;
	onRemove: () => void;
	className?: string;
}) {
	return (
		<span
			className={cn(
				"mx-0.5 inline-flex items-baseline rounded-sm border border-border/60 text-[14px] leading-none transition-colors hover:border-muted-foreground/40 hover:bg-accent/40",
				className,
			)}
		>
			<span className="inline-flex min-w-0 items-baseline gap-1.5 py-[3px] pl-2 pr-1">
				<span className="inline-flex self-center">{icon}</span>
				<span className="max-w-[200px] truncate text-muted-foreground">{label}</span>
			</span>
			<button
				type="button"
				className="mr-1 inline-flex size-4 shrink-0 cursor-pointer items-center justify-center self-center rounded-sm text-muted-foreground/40 transition-colors hover:text-muted-foreground"
				onClick={(e) => {
					e.preventDefault();
					e.stopPropagation();
					onRemove();
				}}
				aria-label="Remove"
			>
				<X className="size-3" strokeWidth={1.8} />
			</button>
		</span>
	);
}
