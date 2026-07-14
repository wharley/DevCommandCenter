import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Badge } from "@/components/ui/badge";
import { InlineShortcutDisplay } from "./InlineShortcutDisplay";
import {
	getCommandPaletteShortcutKeys,
	getFocusComposerShortcutKeys,
	getInspectorCodeModeShortcutKeys,
	getInspectorGitModeShortcutKeys,
	getOpenPreferredEditorShortcutKeys,
	getPrimaryShortcutModifier,
	getQuickOpenShortcutKeys,
	getToggleTerminalShortcutKeys,
	getWorkspaceSearchShortcutKeys,
} from "./shortcut-utils";

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
		() => {
			const modifier = getPrimaryShortcutModifier();
			return [
				{ actionKey: "shortcutsSheet.commandPalette" as const, keys: getCommandPaletteShortcutKeys() },
				{ actionKey: "shortcutsSheet.focusComposer" as const, keys: getFocusComposerShortcutKeys() },
				{ actionKey: "shortcutsSheet.toggleTerminal" as const, keys: getToggleTerminalShortcutKeys() },
				{ actionKey: "shortcutsSheet.quickOpen" as const, keys: getQuickOpenShortcutKeys() },
				{ actionKey: "shortcutsSheet.workspaceSearch" as const, keys: getWorkspaceSearchShortcutKeys() },
				{ actionKey: "shortcutsSheet.inspectorChanges" as const, keys: getInspectorGitModeShortcutKeys() },
				{ actionKey: "shortcutsSheet.inspectorFiles" as const, keys: getInspectorCodeModeShortcutKeys() },
				{ actionKey: "shortcutsSheet.sendPrompt" as const, keys: [modifier, "Enter"] },
				{
					actionKey: "shortcutsSheet.steerSession" as const,
					keys: [modifier, "Shift", "Enter"],
				},
				{ actionKey: "shortcutsSheet.abortSession" as const, keys: ["Esc"] },
				{
					actionKey: "shortcutsSheet.openPreferredEditor" as const,
					keys: getOpenPreferredEditorShortcutKeys(),
				},
			];
		},
		[],
	);

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="max-h-[min(88vh,760px)] w-[min(92vw,560px)] max-w-[560px] overflow-hidden rounded-2xl border-border/60 bg-background p-0 shadow-2xl">
				<div className="flex min-h-0 flex-col p-5">
					<DialogHeader className="space-y-2">
						<Badge variant="outline" className="h-7 px-2.5 text-[11px] font-normal">
							{t("shortcutsSheet.badge")}
						</Badge>
						<DialogTitle className="text-[15px] font-semibold text-foreground">
							{t("shortcutsSheet.title")}
						</DialogTitle>
						<DialogDescription className="sr-only">
							{t("shortcutsSheet.title")}
						</DialogDescription>
					</DialogHeader>
					<div className="mt-5 min-h-0 space-y-2 overflow-y-auto pr-1">
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
