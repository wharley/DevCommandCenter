import * as React from "react";
import * as SeparatorPrimitive from "@radix-ui/react-separator";
import { cn } from "@/lib/utils";

function Separator({
	className,
	orientation = "horizontal",
	decorative = true,
	...props
}: React.ComponentProps<typeof SeparatorPrimitive.Root>) {
	return (
		<SeparatorPrimitive.Root
			decorative={decorative}
			orientation={orientation}
			className={cn(
				orientation === "horizontal" ? "h-px w-full bg-border" : "w-px self-stretch bg-border",
				className,
			)}
			{...props}
		/>
	);
}

export { Separator };
