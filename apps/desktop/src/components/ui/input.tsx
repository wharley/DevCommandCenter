import * as React from "react";
import { cn } from "@/lib/utils";

function Input({
	className,
	type = "text",
	...props
}: React.ComponentProps<"input">) {
	return <input type={type} className={cn("dcc-input", className)} {...props} />;
}

export { Input };
