import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Badge } from "@/components/ui/badge";
import { InlineShortcutDisplay } from "./InlineShortcutDisplay";

type ShortcutCheatsheetDialogProps = {
	open: boolean;
	onOpenChange: (open: boolean) => void;
};

const shortcutRows = [
	{ action: "Open command palette", keys: ["Cmd", "K"] },
	{ action: "Send prompt", keys: ["Cmd", "Enter"] },
	{ action: "Steer session", keys: ["Cmd", "Shift", "Enter"] },
	{ action: "Abort session", keys: ["Esc"] },
];

export function ShortcutCheatsheetDialog({
	open,
	onOpenChange,
}: ShortcutCheatsheetDialogProps) {
	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="w-[min(92vw,520px)] max-w-[520px] rounded-2xl border-border/60 bg-background p-0 shadow-2xl">
				<div className="p-5">
					<DialogHeader className="space-y-2">
						<Badge variant="outline" className="h-7 px-2.5 text-[11px] font-normal">
							Shortcuts
						</Badge>
						<DialogTitle className="text-[15px] font-semibold text-foreground">
							Keyboard shortcuts
						</DialogTitle>
					</DialogHeader>
					<div className="mt-5 space-y-2">
						{shortcutRows.map((row) => (
							<div
								key={row.action}
								className="flex items-center justify-between gap-4 rounded-xl border border-border/60 px-4 py-3"
							>
								<div>
									<p className="text-[13px] font-medium text-foreground">{row.action}</p>
								</div>
								<InlineShortcutDisplay keys={row.keys} />
							</div>
						))}
					</div>
				</div>
			</DialogContent>
		</Dialog>
	);
}
