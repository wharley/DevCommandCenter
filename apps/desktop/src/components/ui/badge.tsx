import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

const badgeVariants = cva("dcc-badge", {
	variants: {
		variant: {
			default: "dcc-badge--default",
			secondary: "dcc-badge--secondary",
			outline: "dcc-badge--outline",
			success: "dcc-badge--success",
			warn: "dcc-badge--warn",
		},
	},
	defaultVariants: {
		variant: "default",
	},
});

function Badge({
	className,
	variant,
	...props
}: React.HTMLAttributes<HTMLDivElement> & VariantProps<typeof badgeVariants>) {
	return <div className={cn(badgeVariants({ variant }), className)} {...props} />;
}

export { Badge, badgeVariants };
