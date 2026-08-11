import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";

type SyncBaseDialogProps = {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	baseBranch: string | null;
	loading?: boolean;
	onConfirm: () => void;
};

export function SyncBaseDialog({
	open,
	onOpenChange,
	baseBranch,
	loading = false,
	onConfirm,
}: SyncBaseDialogProps) {
	const { t } = useTranslation("common");

	return (
		<Dialog open={open} onOpenChange={(next) => !loading && onOpenChange(next)}>
			<DialogContent>
				<DialogHeader>
					<DialogTitle>{t("inspector.gitConfirmation.syncTitle")}</DialogTitle>
					<DialogDescription>
						{baseBranch
							? t("inspector.gitConfirmation.syncDescriptionWithBase", { baseBranch })
							: t("inspector.gitConfirmation.syncDescription")}
					</DialogDescription>
				</DialogHeader>
				<DialogFooter>
					<Button type="button" variant="outline" disabled={loading} onClick={() => onOpenChange(false)}>
						{t("inspector.gitConfirmation.cancel")}
					</Button>
					<Button type="button" disabled={loading} onClick={onConfirm}>
						{t("inspector.gitConfirmation.syncConfirm")}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
