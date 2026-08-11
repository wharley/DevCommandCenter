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

type MergeConfirmDialogProps = {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	requestLabel: "PR" | "MR";
	loading?: boolean;
	onConfirm: () => void;
};

export function MergeConfirmDialog({
	open,
	onOpenChange,
	requestLabel,
	loading = false,
	onConfirm,
}: MergeConfirmDialogProps) {
	const { t } = useTranslation("common");

	return (
		<Dialog open={open} onOpenChange={(next) => !loading && onOpenChange(next)}>
			<DialogContent>
				<DialogHeader>
					<DialogTitle>
						{t("inspector.gitConfirmation.mergeTitle", { requestLabel })}
					</DialogTitle>
					<DialogDescription>
						{t("inspector.gitConfirmation.mergeDescription", { requestLabel })}
					</DialogDescription>
				</DialogHeader>
				<DialogFooter>
					<Button type="button" variant="outline" disabled={loading} onClick={() => onOpenChange(false)}>
						{t("inspector.gitConfirmation.cancel")}
					</Button>
					<Button type="button" disabled={loading} onClick={onConfirm}>
						{t("inspector.gitConfirmation.mergeConfirm", { requestLabel })}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
