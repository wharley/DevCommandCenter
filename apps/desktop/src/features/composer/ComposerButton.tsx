import type * as React from "react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { composerToolbarTriggerClassName } from "./WorkspaceComposer.logic";

type ComposerButtonProps = React.ComponentProps<typeof Button> & {
	active?: boolean;
};

export function ComposerButton({
	active,
	className,
	children,
	...props
}: ComposerButtonProps) {
	return (
		<Button
			type="button"
			variant="ghost"
			size="sm"
			className={cn(
				composerToolbarTriggerClassName,
				"inline-flex h-7 items-center gap-1.5 rounded-[9px] px-2",
				active && "bg-accent/60 text-foreground",
				className,
			)}
			{...props}
		>
			{children}
		</Button>
	);
}
