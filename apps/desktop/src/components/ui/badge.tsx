import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

const badgeVariants = cva(
	"group/badge inline-flex h-5 w-fit shrink-0 items-center justify-center gap-1 overflow-hidden rounded-4xl border border-transparent px-2 py-0.5 text-xs font-medium whitespace-nowrap transition-all focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50",
	{
		variants: {
			variant: {
				default: "bg-primary text-primary-foreground",
				secondary: "bg-secondary text-secondary-foreground",
				destructive: "bg-destructive/10 text-destructive",
				outline: "border-border bg-transparent text-foreground",
				ghost: "bg-transparent text-foreground",
				link: "bg-transparent text-primary underline-offset-4 hover:underline",
				success:
					"bg-[color-mix(in_oklab,var(--success)_20%,var(--secondary))] text-foreground",
				warn:
					"bg-[color-mix(in_oklab,var(--warning)_20%,var(--secondary))] text-foreground",
			},
		},
		defaultVariants: {
			variant: "default",
		},
	},
);

function Badge({
	className,
	variant,
	...props
}: React.HTMLAttributes<HTMLDivElement> & VariantProps<typeof badgeVariants>) {
	return <div className={cn(badgeVariants({ variant }), className)} {...props} />;
}

export { Badge, badgeVariants };
