import { FolderOpen, Link2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

type WorkspaceBootstrapStateProps = {
	selectedProviderLabel: string | null;
	selectedModelLabel: string | null;
	onCreateWorkspace: () => void;
	onCloneWorkspace: () => void;
};

export function WorkspaceBootstrapState({
	selectedProviderLabel,
	selectedModelLabel,
	onCreateWorkspace,
	onCloneWorkspace,
}: WorkspaceBootstrapStateProps) {
	const { t } = useTranslation("common");
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
					{t("bootstrap.title")}
				</h3>
				<p className="mt-2 max-w-lg text-[13px] leading-6 text-muted-foreground">
					{t("bootstrap.description")}
				</p>
				<div className="mt-6 flex flex-wrap items-center justify-center gap-2">
					<Button type="button" size="sm" className="gap-1.5" onClick={onCreateWorkspace}>
						<FolderOpen className="size-3.5" strokeWidth={2} aria-hidden />
						{t("bootstrap.openProject")}
					</Button>
					<Button
						type="button"
						size="sm"
						variant="outline"
						className="gap-1.5"
						onClick={onCloneWorkspace}
					>
						<Link2 className="size-3.5" strokeWidth={2} aria-hidden />
						{t("bootstrap.cloneFromUrl")}
					</Button>
				</div>
				{selectedProviderLabel ? (
					<p className="mt-4 text-[12px] text-muted-foreground">
						{t("bootstrap.currentProvider", {
							provider: selectedProviderLabel,
							model: selectedModelLabel ? ` · ${selectedModelLabel}` : "",
						})}
					</p>
				) : null}
			</div>
		</div>
	);
}
