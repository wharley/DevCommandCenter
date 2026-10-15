import { FolderTree } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { InlineShortcutDisplay } from "@/features/shortcuts/InlineShortcutDisplay";
import { getCodeColumnShortcutKeys } from "@/features/shortcuts/shortcut-utils";
import { cn } from "@/lib/utils";
import { toggleCodeColumn, useCodeColumnOpen } from "./code-column-store";

/** Shows or hides the code column; lives on the sidebar it opens next to. */
export function CodeColumnToggle({
	tooltipSide = "bottom",
}: {
	tooltipSide?: "bottom" | "right";
}) {
	const { t } = useTranslation("common");
	const open = useCodeColumnOpen();
	const label = open ? t("codeColumn.close") : t("codeColumn.open");
	return (
		<Tooltip>
			<TooltipTrigger asChild>
				<Button
					type="button"
					variant="ghost"
					size="icon-xs"
					onClick={toggleCodeColumn}
					aria-label={label}
					aria-pressed={open}
					className={cn(
						open ? "text-foreground" : "text-muted-foreground hover:text-foreground",
					)}
				>
					<FolderTree className="size-4" strokeWidth={1.8} />
				</Button>
			</TooltipTrigger>
			<TooltipContent side={tooltipSide} className="gap-2">
				<span>{label}</span>
				<InlineShortcutDisplay keys={getCodeColumnShortcutKeys()} />
			</TooltipContent>
		</Tooltip>
	);
}
