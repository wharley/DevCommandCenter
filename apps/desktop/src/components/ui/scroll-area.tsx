import * as React from "react";
import * as ScrollAreaPrimitive from "@radix-ui/react-scroll-area";
import { cn } from "@/lib/utils";

function ScrollArea({
	className,
	children,
	...props
}: React.ComponentProps<typeof ScrollAreaPrimitive.Root>) {
	return (
		<ScrollAreaPrimitive.Root className={cn("dcc-scroll-area", className)} {...props}>
			<ScrollAreaPrimitive.Viewport className="dcc-scroll-area-viewport">
				{children}
			</ScrollAreaPrimitive.Viewport>
			<ScrollAreaPrimitive.Scrollbar
				orientation="vertical"
				className="dcc-scroll-area-scrollbar"
			>
				<ScrollAreaPrimitive.Thumb className="dcc-scroll-area-thumb" />
			</ScrollAreaPrimitive.Scrollbar>
			<ScrollAreaPrimitive.Corner />
		</ScrollAreaPrimitive.Root>
	);
}

export { ScrollArea };
