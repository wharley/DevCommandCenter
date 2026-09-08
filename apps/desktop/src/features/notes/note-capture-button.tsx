import { Lightbulb } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import "./notes.css";

/** Lives in the composer footer's flex layout, so it cannot cover the branch. */
export function NoteCaptureButton({ sessionId }: { sessionId: string | null }) {
	const { t } = useTranslation("common");
	return (
		<Tooltip>
			<TooltipTrigger asChild>
				<button
					type="button"
					className="notes-capture-button"
					aria-label={t("notes.capture")}
					onPointerDown={(event) => event.preventDefault()}
					onClick={() =>
						window.dispatchEvent(
							new CustomEvent("dcc:capture-note", {
								detail: {
									sessionId,
									content: window.getSelection()?.toString() ?? "",
								},
							}),
						)
					}
				>
					<Lightbulb size={16} aria-hidden />
				</button>
			</TooltipTrigger>
			<TooltipContent side="top" align="end" sideOffset={10}>
				{t("notes.captureShortcut")}
			</TooltipContent>
		</Tooltip>
	);
}
