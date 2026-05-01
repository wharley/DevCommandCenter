import { formatDistanceToNow } from "date-fns";

export function MessageTimestamp({ createdAt }: { createdAt?: string }) {
	if (!createdAt) {
		return null;
	}

	const date = new Date(createdAt);
	if (Number.isNaN(date.getTime())) {
		return null;
	}

	return (
		<span className="inline-flex h-4 shrink-0 items-center text-[11px] leading-none tabular-nums text-muted-foreground">
			{formatDistanceToNow(date, { addSuffix: true })}
		</span>
	);
}
