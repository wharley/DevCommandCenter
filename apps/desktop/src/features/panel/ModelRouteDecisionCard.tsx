import { Route } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";

export type ModelRouteDecision = {
	currentModel: string | null;
	recommendedModel: string;
	recommendedScore: number | null;
	confidence: number | null;
};

type ModelRouteDecisionCardProps = {
	decision: ModelRouteDecision;
	modelLabel: (modelId: string | null) => string;
	onSelect: (modelId: string) => void;
};

export function ModelRouteDecisionCard({
	decision,
	modelLabel,
	onSelect,
}: ModelRouteDecisionCardProps) {
	const { t } = useTranslation("common");
	const currentLabel = modelLabel(decision.currentModel);
	const recommendedLabel = modelLabel(decision.recommendedModel);

	return (
		<div
			role="status"
			aria-live="polite"
			className="conversation-thread-enter conversation-fade-in flex min-w-0 justify-start px-5 py-6 motion-reduce:animate-none"
		>
			<div className="w-full max-w-[31rem] rounded-xl border border-border/60 bg-muted/15 px-3.5 py-3 shadow-[0_1px_3px_rgb(0_0_0/0.025)]">
				<div className="flex items-start gap-2">
					<Route className="mt-0.5 size-4 shrink-0 text-muted-foreground" aria-hidden />
					<div className="min-w-0">
						<div className="text-[12px] font-medium text-foreground/90">
							{t("conversation.modelRouting.title")}
						</div>
						<p className="mt-1 text-[11px] leading-[17px] text-muted-foreground">
							{t("conversation.modelRouting.description", {
								current: currentLabel,
								recommended: recommendedLabel,
							})}
						</p>
						{decision.recommendedScore !== null ? (
							<p className="mt-1 text-[10px] text-muted-foreground/80">
								{t("conversation.modelRouting.score", {
									score: Math.round(decision.recommendedScore * 100),
									confidence: decision.confidence === null
										? "—"
										: Math.round(decision.confidence * 100),
								})}
							</p>
						) : null}
					</div>
				</div>
				<div className="mt-3 flex flex-wrap gap-2">
					<Button type="button" size="sm" onClick={() => onSelect(decision.recommendedModel)}>
						{t("conversation.modelRouting.useRecommended", { model: recommendedLabel })}
					</Button>
					<Button
						type="button"
						variant="outline"
						size="sm"
						onClick={() => onSelect(decision.currentModel ?? decision.recommendedModel)}
					>
						{t("conversation.modelRouting.keepCurrent", { model: currentLabel })}
					</Button>
				</div>
			</div>
		</div>
	);
}
