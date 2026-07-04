import type { Delegation } from "@dcc/contracts";

export function delegationStatusClass(status: Delegation["status"]) {
	switch (status) {
		case "completed":
			return "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300";
		case "failed":
			return "border-destructive/30 bg-destructive/10 text-destructive";
		case "cancelled":
			return "border-muted-foreground/30 bg-muted/30 text-muted-foreground";
		case "running":
			return "border-sky-500/30 bg-sky-500/10 text-sky-700 dark:text-sky-300";
		case "review_pending":
			return "border-amber-500/30 bg-amber-500/10 text-amber-700 dark:text-amber-300";
		default:
			return "border-border bg-muted/30 text-muted-foreground";
	}
}
