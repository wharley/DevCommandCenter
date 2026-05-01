import * as React from "react";
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

const buttonVariants = cva("", {
	variants: {
		variant: {
			default:
				"bg-primary text-primary-foreground [a]:hover:bg-primary/80",
			secondary: "bg-secondary text-secondary-foreground hover:bg-secondary/80",
			ghost:
				"hover:bg-muted hover:text-foreground aria-expanded:bg-muted aria-expanded:text-foreground dark:hover:bg-muted/50",
			outline:
				"border-border bg-background hover:bg-muted hover:text-foreground aria-expanded:bg-muted aria-expanded:text-foreground dark:border-input dark:bg-input/30 dark:hover:bg-input/50",
			destructive:
				"bg-destructive/10 text-destructive hover:bg-destructive/20 focus-visible:border-destructive/40 focus-visible:ring-destructive/20 dark:bg-destructive/20 dark:hover:bg-destructive/30 dark:focus-visible:ring-destructive/40",
			link: "text-primary underline-offset-4 hover:underline",
		},
		size: {
			default: "h-8 px-3",
			xs: "h-6 px-2 text-[12px]",
			sm: "h-7 px-2.5",
			lg: "h-9 px-4",
			icon: "size-8",
			"icon-xs": "size-6",
			"icon-sm": "size-7",
			"icon-lg": "size-9",
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
			className={cn(
				"group/button inline-flex shrink-0 cursor-pointer items-center justify-center rounded-lg border border-transparent bg-clip-padding in-data-[slot=button-group]:bg-clip-border text-sm font-medium whitespace-nowrap transition-all outline-none select-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 active:not-aria-[haspopup]:translate-y-px disabled:pointer-events-none disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-3 aria-invalid:ring-destructive/20 dark:aria-invalid:border-destructive/50 dark:aria-invalid:ring-destructive/40 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
				buttonVariants({ variant, size }),
				className,
			)}
			{...props}
		/>
	);
}

export { Button, buttonVariants };
