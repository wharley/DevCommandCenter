import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";

type FileViewMode = "diff" | "file";

/**
 * Quiet segmented control that flips a file surface between its diff view and the
 * read-only whole-file view. The active segment is inert; the other one triggers
 * the switch. Shared by both surfaces so the control reads identically in either.
 */
export function FileViewToggle({
	mode,
	onSelectDiff,
	onSelectWholeFile,
}: {
	mode: FileViewMode;
	onSelectDiff?: () => void;
	onSelectWholeFile?: () => void;
}) {
	const { t } = useTranslation("common");

	return (
		<div
			role="group"
			aria-label={t("fileSurface.ariaLabel")}
			className="inline-flex items-center gap-0.5 rounded-md border border-border/60 bg-muted/30 p-0.5"
		>
			<Segment
				active={mode === "diff"}
				label={t("fileSurface.viewDiff")}
				onClick={onSelectDiff}
			/>
			<Segment
				active={mode === "file"}
				label={t("fileSurface.viewWholeFile")}
				onClick={onSelectWholeFile}
			/>
		</div>
	);
}

function Segment({
	active,
	label,
	onClick,
}: {
	active: boolean;
	label: string;
	onClick?: () => void;
}) {
	return (
		<button
			type="button"
			aria-pressed={active}
			disabled={active}
			onClick={onClick}
			className={cn(
				"rounded px-2 py-0.5 text-[11px] font-medium transition-colors",
				"focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
				active
					? "bg-background text-foreground shadow-sm"
					: "text-muted-foreground hover:text-foreground",
			)}
		>
			{label}
		</button>
	);
}
