import * as LabelPrimitive from "@radix-ui/react-label";
import type * as React from "react";
import { cn } from "@/lib/utils";

function Label({
	className,
	...props
}: React.ComponentProps<typeof LabelPrimitive.Root>) {
	return (
		<LabelPrimitive.Root
			data-slot="label"
			className={cn(
				"flex cursor-pointer items-center gap-2 text-sm font-medium leading-none select-none peer-disabled:pointer-events-none peer-disabled:opacity-50",
				className,
			)}
			{...props}
		/>
	);
}

export { Label };
