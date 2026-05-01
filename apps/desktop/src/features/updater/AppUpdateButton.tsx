import { Download, Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

type AppUpdateButtonProps = {
	update:
		| {
				currentVersion: string;
				version: string;
				stage: "downloaded" | "checking" | "idle";
		  }
		| null;
	installing?: boolean;
	onInstallNow: () => void;
};

export function AppUpdateButton({
	update,
	installing = false,
	onInstallNow,
}: AppUpdateButtonProps) {
	if (!update || update.stage !== "downloaded") {
		return null;
	}

	return (
		<Tooltip>
			<TooltipTrigger asChild>
				<Button
					type="button"
					variant="ghost"
					size="xs"
					className="relative h-6 gap-1 overflow-hidden rounded-sm px-1.5 text-[11px] font-medium tracking-[0.01em] text-muted-foreground shadow-[inset_0_0_0_1px_color-mix(in_srgb,var(--border)_36%,transparent)] hover:bg-accent/60 hover:text-foreground hover:shadow-[inset_0_0_0_1px_color-mix(in_srgb,var(--foreground)_12%,transparent)] dark:hover:bg-muted/45"
					disabled={installing}
					onClick={onInstallNow}
				>
					{installing ? (
						<Loader2 className="size-3 animate-spin text-foreground/70" />
					) : (
						<Download className="size-3 text-foreground/72" />
					)}
					<span>Update</span>
				</Button>
			</TooltipTrigger>
			<TooltipContent
				side="top"
				sideOffset={4}
				className="flex h-[22px] items-center gap-1 rounded-md px-1.5 text-[11px] leading-none"
			>
				{update.currentVersion} → {update.version}
			</TooltipContent>
		</Tooltip>
	);
}
