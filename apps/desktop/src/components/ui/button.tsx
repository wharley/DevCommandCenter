import * as React from "react";
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

const buttonVariants = cva("dcc-button", {
	variants: {
		variant: {
			default: "dcc-button--default",
			secondary: "dcc-button--secondary",
			ghost: "dcc-button--ghost",
			outline: "dcc-button--outline",
			destructive: "dcc-button--destructive",
			link: "dcc-button--link",
		},
		size: {
			default: "",
			sm: "dcc-button--sm",
			lg: "dcc-button--lg",
			icon: "dcc-button--icon",
			"icon-sm": "dcc-button--icon-sm",
			"icon-xs": "dcc-button--icon-xs",
		},
	},
	defaultVariants: {
		variant: "default",
		size: "default",
	},
});

type ButtonProps = React.ButtonHTMLAttributes<HTMLButtonElement> &
	VariantProps<typeof buttonVariants> & {
		asChild?: boolean;
	};

function Button({
	className,
	variant,
	size,
	asChild = false,
	...props
}: ButtonProps) {
	const Component = asChild ? Slot : "button";

	return (
		<Component
			data-slot="button"
			data-variant={variant}
			data-size={size}
			className={cn(buttonVariants({ variant, size }), className)}
			{...props}
		/>
	);
}

export { Button, buttonVariants };
