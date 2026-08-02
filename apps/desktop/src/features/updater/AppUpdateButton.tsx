import { Download, Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

type AppUpdateButtonProps = {
	update:
		| {
				currentVersion: string;
				version: string;
				stage: "available" | "checking" | "idle";
		  }
		| null;
	installing?: boolean;
	collapsed?: boolean;
	className?: string;
	onInstallNow: () => void;
};

export function AppUpdateButton({
	update,
	installing = false,
	collapsed = false,
	className,
	onInstallNow,
}: AppUpdateButtonProps) {
	const { t } = useTranslation("common");
	if (!update || update.stage !== "available") {
		return null;
	}

	const button = (
		<Button
			type="button"
			variant="ghost"
			size={collapsed ? "icon-xs" : "sm"}
			className={cn(
				"group/update relative shrink-0 justify-start gap-0 overflow-hidden rounded-lg bg-sky-500 px-2 text-[11px] font-medium tracking-[0.01em] text-white shadow-sm transition-[max-width,background-color] duration-200 hover:bg-sky-400 hover:text-white dark:hover:bg-sky-400",
				collapsed ? "size-7 justify-center px-0" : "h-8 max-w-8 hover:max-w-28",
				className,
			)}
			aria-label={installing ? t("updater.installing") : t("updater.update")}
			disabled={installing}
			onClick={onInstallNow}
		>
			{installing ? (
				<Loader2 className="size-4 shrink-0 animate-spin" />
			) : (
				<Download className="size-4 shrink-0" strokeWidth={2.1} />
			)}
			{collapsed ? null : (
				<span className="ml-0 max-w-0 overflow-hidden whitespace-nowrap opacity-0 transition-[max-width,margin,opacity] duration-200 group-hover/update:ml-1.5 group-hover/update:max-w-20 group-hover/update:opacity-100">
					{installing ? t("updater.installing") : t("updater.update")}
				</span>
			)}
		</Button>
	);

	if (!collapsed) {
		return button;
	}

	return (
		<Tooltip>
			<TooltipTrigger asChild>{button}</TooltipTrigger>
			<TooltipContent
				side="right"
				sideOffset={4}
				className="flex h-[22px] items-center gap-1 rounded-md px-1.5 text-[11px] leading-none"
			>
				{t("updater.update")} · {update.currentVersion} → {update.version}
			</TooltipContent>
		</Tooltip>
	);
}
