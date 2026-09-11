import { MessageSquareMore } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@/components/ui/tooltip";

export function FeedbackButton({
	onClick,
	collapsed = false,
}: {
	onClick: () => void;
	collapsed?: boolean;
}) {
	const { t } = useTranslation("common");
	return (
		<Tooltip>
			<TooltipTrigger asChild>
				<Button
					type="button"
					variant="ghost"
					size="icon"
					className="text-muted-foreground hover:text-foreground"
					aria-label={t("feedback.title")}
					onClick={onClick}
				>
					<MessageSquareMore
						className="size-4"
						strokeWidth={1.85}
						aria-hidden
					/>
				</Button>
			</TooltipTrigger>
			<TooltipContent side={collapsed ? "right" : "top"}>
				{t("feedback.title")}
			</TooltipContent>
		</Tooltip>
	);
}
