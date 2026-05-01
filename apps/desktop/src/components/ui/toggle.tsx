import * as TogglePrimitive from "@radix-ui/react-toggle";
import { cva, type VariantProps } from "class-variance-authority";
import type * as React from "react";
import { cn } from "@/lib/utils";

const toggleVariants = cva(
	"group/toggle inline-flex cursor-pointer items-center justify-center gap-1 rounded-lg text-sm font-medium whitespace-nowrap transition-all outline-none hover:bg-muted hover:text-foreground focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50 aria-pressed:bg-muted data-[state=on]:bg-muted",
	{
		variants: {
			size: {
				default: "h-8 px-3",
				sm: "h-7 px-2.5",
				icon: "size-8",
			},
		},
		defaultVariants: {
			size: "default",
		},
	},
);

function Toggle({
	className,
	size = "default",
	...props
}: React.ComponentProps<typeof TogglePrimitive.Root> &
	VariantProps<typeof toggleVariants>) {
	return (
		<TogglePrimitive.Root
			data-slot="toggle"
			data-size={size}
			className={cn(toggleVariants({ size }), className)}
			{...props}
		/>
	);
}

export { Toggle, toggleVariants };
