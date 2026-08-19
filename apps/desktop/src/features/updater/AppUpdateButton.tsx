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
			size={collapsed ? "icon-sm" : "icon"}
			className={cn(
				"relative shrink-0 rounded-lg bg-sky-500 text-white shadow-sm hover:bg-sky-400 hover:text-white dark:hover:bg-sky-400",
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
		</Button>
	);

	return (
		<Tooltip>
			<TooltipTrigger asChild>{button}</TooltipTrigger>
			<TooltipContent
				side={collapsed ? "right" : "top"}
				sideOffset={4}
				className="flex h-[22px] items-center gap-1 rounded-md px-1.5 text-[11px] leading-none"
			>
				{t("updater.update")} · {update.currentVersion} → {update.version}
			</TooltipContent>
		</Tooltip>
	);
}
