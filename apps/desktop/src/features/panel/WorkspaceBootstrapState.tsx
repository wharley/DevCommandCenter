import { FolderOpen, Link2, Command as CommandIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

type WorkspaceBootstrapStateProps = {
	selectedProviderLabel: string | null;
	selectedModelLabel: string | null;
	onCreateWorkspace: () => void;
	onCloneWorkspace: () => void;
	onOpenCommandPalette: () => void;
};

export function WorkspaceBootstrapState({
	selectedProviderLabel,
	selectedModelLabel,
	onCreateWorkspace,
	onCloneWorkspace,
	onOpenCommandPalette,
}: WorkspaceBootstrapStateProps) {
	return (
		<div className="flex min-h-0 flex-1 items-center justify-center px-6 py-10">
			<div className="flex w-full max-w-2xl flex-col items-center text-center">
				<div
					className={cn(
						"mb-4 flex size-12 items-center justify-center rounded-full border border-border/70 bg-muted/20 text-muted-foreground",
					)}
				>
					<FolderOpen className="size-5" aria-hidden />
				</div>
				<h3 className="text-[15px] font-medium tracking-[-0.01em] text-foreground">
					No workspace open
				</h3>
				<p className="mt-2 max-w-lg text-[13px] leading-6 text-muted-foreground">
					Open a project or clone from URL to start from zero. The chat, inspector,
					and terminal surfaces will appear once a workspace exists.
				</p>
				<div className="mt-6 flex flex-wrap items-center justify-center gap-2">
					<Button type="button" size="sm" className="gap-1.5" onClick={onCreateWorkspace}>
						<FolderOpen className="size-3.5" strokeWidth={2} aria-hidden />
						Open project
					</Button>
					<Button
						type="button"
						size="sm"
						variant="outline"
						className="gap-1.5"
						onClick={onCloneWorkspace}
					>
						<Link2 className="size-3.5" strokeWidth={2} aria-hidden />
						Clone from URL
					</Button>
					<Button
						type="button"
						size="sm"
						variant="ghost"
						className="gap-1.5 text-muted-foreground hover:text-foreground"
						onClick={onOpenCommandPalette}
					>
						<CommandIcon className="size-3.5" strokeWidth={2} aria-hidden />
						Command palette
					</Button>
				</div>
				{selectedProviderLabel ? (
					<p className="mt-4 text-[12px] text-muted-foreground">
						Current provider: {selectedProviderLabel}
						{selectedModelLabel ? ` · ${selectedModelLabel}` : ""}
					</p>
				) : null}
			</div>
		</div>
	);
}
