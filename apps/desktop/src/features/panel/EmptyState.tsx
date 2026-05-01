import { CircleSlash2 } from "lucide-react";

export function EmptyState({
	title,
	description,
}: {
	title: string;
	description: string;
}) {
	return (
		<div className="flex min-h-full flex-1 items-center justify-center px-8">
			<div className="flex max-w-md flex-col items-center text-center">
				<div className="mb-3 flex size-11 items-center justify-center rounded-full border border-border/70 bg-muted/20 text-muted-foreground">
					<CircleSlash2 className="size-5" aria-hidden />
				</div>
				<h3 className="text-[15px] font-medium tracking-[-0.01em] text-foreground">
					{title}
				</h3>
				<p className="mt-2 text-[13px] leading-6 text-muted-foreground">
					{description}
				</p>
			</div>
		</div>
	);
}
