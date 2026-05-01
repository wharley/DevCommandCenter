import { X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { ComposerContextDirectory } from "./WorkspaceComposer.logic";
import { formatContextLabel } from "./ContextBar.logic";

type ContextBarProps = {
	directories: ComposerContextDirectory[];
	disabled?: boolean;
	onRemove?: (directoryId: string) => void;
};

export function ContextBar({
	directories,
	disabled = false,
	onRemove,
}: ContextBarProps) {
	const hasOverflow = directories.length > 2;

	return (
		<div data-slot="context-bar" className="relative -mx-4 mb-2">
			<div className="flex items-center border-b border-dashed border-border/55 px-4 pb-2 pt-0.5">
				<span className="shrink-0 pr-2 font-mono text-[10.5px] uppercase tracking-[0.06em] text-muted-foreground">
					context
				</span>
				<div className="relative min-w-0 flex-1">
					<div
						aria-hidden
						className={cn(
							"pointer-events-none absolute inset-y-0 right-0 w-12 bg-gradient-to-r from-transparent to-sidebar transition-opacity duration-200",
							hasOverflow ? "opacity-100" : "opacity-0",
						)}
					/>
					<div className="scrollbar-none flex items-center gap-1 overflow-x-auto">
						{directories.length === 0 ? (
							<span className="text-[12px] text-muted-foreground/70">
								No extra context
							</span>
						) : (
							directories.map((directory, index) => (
								<span
									key={directory.id}
									role="listitem"
									className={cn(
										"group/chip inline-flex shrink-0 items-center gap-1.5 rounded-md px-1.5 py-0.5 text-[12px] leading-tight outline-none transition-colors hover:bg-accent/60 focus-visible:bg-accent/60 focus-visible:shadow-[0_0_0_2px_color-mix(in_srgb,var(--workspace-pr-merged-accent)_35%,transparent)]",
										disabled && "opacity-60",
										index > 0 && "ml-0.5",
									)}
								>
									<span className="inline-flex min-w-0 items-baseline gap-1.5 py-[3px] pl-2 pr-1">
										<span className="max-w-[200px] truncate text-muted-foreground">
											{formatContextLabel(directory)}
										</span>
									</span>
									{onRemove ? (
										<Button
											type="button"
											variant="ghost"
											size="icon-xs"
											className="mr-1 inline-flex size-4 shrink-0 cursor-pointer items-center justify-center self-center rounded-sm text-muted-foreground/40 transition-colors hover:text-muted-foreground"
											disabled={disabled}
											aria-label={`Remove ${directory.label}`}
											onClick={() => onRemove(directory.id)}
										>
											<X className="size-3" strokeWidth={1.8} />
										</Button>
									) : null}
								</span>
							))
						)}
					</div>
				</div>
			</div>
		</div>
	);
}
