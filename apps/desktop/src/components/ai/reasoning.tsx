import type * as React from "react";
import { ChevronRight, Brain } from "lucide-react";
import { useState } from "react";
import { cn } from "@/lib/utils";

export function Reasoning({
	children,
	label,
	defaultOpen = false,
}: {
	children: React.ReactNode;
	label?: string;
	defaultOpen?: boolean;
}) {
	const [isOpen, setIsOpen] = useState(defaultOpen);

	return (
		<details
			className="group/reasoning flex flex-col"
			open={isOpen}
			onToggle={(event) => setIsOpen(event.currentTarget.open)}
		>
			<summary
				className={cn(
					"group/reasoning-trigger inline-flex max-w-full cursor-pointer items-center gap-1.5 py-0.5 text-[12px] text-muted-foreground transition-colors hover:text-foreground [&::-webkit-details-marker]:hidden",
				)}
			>
				<Brain className="size-3 shrink-0" aria-hidden />
				<span className="min-w-0 truncate">
					{label ?? "Thinking"}
				</span>
				<ChevronRight
					className={cn("size-3 shrink-0 transition-transform", isOpen && "rotate-90")}
					aria-hidden
				/>
			</summary>
			<div className="pl-4 pt-1 text-[12px] leading-6 text-muted-foreground">
				{children}
			</div>
		</details>
	);
}
