import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Badge } from "@/components/ui/badge";
import { InlineShortcutDisplay } from "./InlineShortcutDisplay";
import { getOpenPreferredEditorShortcutKeys } from "./shortcut-utils";

type ShortcutCheatsheetDialogProps = {
	open: boolean;
	onOpenChange: (open: boolean) => void;
};

export function ShortcutCheatsheetDialog({
	open,
	onOpenChange,
}: ShortcutCheatsheetDialogProps) {
	const { t } = useTranslation("common");
	const shortcutRows = useMemo(
		() =>
			[
				{ actionKey: "shortcutsSheet.sendPrompt" as const, keys: ["Cmd", "Enter"] as const },
				{
					actionKey: "shortcutsSheet.steerSession" as const,
					keys: ["Cmd", "Shift", "Enter"] as const,
				},
				{ actionKey: "shortcutsSheet.abortSession" as const, keys: ["Esc"] as const },
				{
					actionKey: "shortcutsSheet.openPreferredEditor" as const,
					keys: getOpenPreferredEditorShortcutKeys(),
				},
			],
		[],
	);

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="w-[min(92vw,520px)] max-w-[520px] rounded-2xl border-border/60 bg-background p-0 shadow-2xl">
				<div className="p-5">
					<DialogHeader className="space-y-2">
						<Badge variant="outline" className="h-7 px-2.5 text-[11px] font-normal">
							{t("shortcutsSheet.badge")}
						</Badge>
						<DialogTitle className="text-[15px] font-semibold text-foreground">
							{t("shortcutsSheet.title")}
						</DialogTitle>
					</DialogHeader>
					<div className="mt-5 space-y-2">
						{shortcutRows.map((row) => (
							<div
								key={row.actionKey}
								className="flex items-center justify-between gap-4 rounded-xl border border-border/60 px-4 py-3"
							>
								<div>
									<p className="text-[13px] font-medium text-foreground">{t(row.actionKey)}</p>
								</div>
								<InlineShortcutDisplay keys={[...row.keys]} />
							</div>
						))}
					</div>
				</div>
			</DialogContent>
		</Dialog>
	);
}
