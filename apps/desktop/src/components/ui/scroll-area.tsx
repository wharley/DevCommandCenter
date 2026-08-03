import * as React from "react";
import * as ScrollAreaPrimitive from "@radix-ui/react-scroll-area";
import { cn } from "@/lib/utils";

type ScrollAreaProps = React.ComponentProps<typeof ScrollAreaPrimitive.Root> & {
	viewportProps?: React.ComponentProps<typeof ScrollAreaPrimitive.Viewport> & {
		"data-inspector-scroll-key"?: string;
	};
};

function ScrollArea({
	className,
	children,
	viewportProps,
	...props
}: ScrollAreaProps) {
	const { className: viewportClassName, ...restViewportProps } =
		viewportProps ?? {};
	return (
		<ScrollAreaPrimitive.Root className={cn("relative overflow-hidden", className)} {...props}>
			<ScrollAreaPrimitive.Viewport
				{...restViewportProps}
				className={cn(
					"size-full rounded-[inherit] transition-[color,box-shadow] outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50 [&>div]:!block [&>div]:!w-full [&>div]:!min-w-0",
					viewportClassName,
				)}
			>
				{children}
			</ScrollAreaPrimitive.Viewport>
			<ScrollAreaPrimitive.Scrollbar
				orientation="vertical"
				className="flex touch-none p-px transition-colors select-none data-[orientation=horizontal]:h-2.5 data-[orientation=vertical]:w-2.5"
			>
				<ScrollAreaPrimitive.Thumb className="relative flex-1 rounded-full bg-border" />
			</ScrollAreaPrimitive.Scrollbar>
			<ScrollAreaPrimitive.Corner />
		</ScrollAreaPrimitive.Root>
	);
}

export { ScrollArea };
