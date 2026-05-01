import * as React from "react";
import { cn } from "@/lib/utils";

function Textarea({
	className,
	...props
}: React.ComponentProps<"textarea">) {
	return <textarea className={cn("dcc-textarea", className)} {...props} />;
}

export { Textarea };
