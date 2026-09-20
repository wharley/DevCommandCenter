import type { DecisionEvaluation } from "@dcc/contracts";
import { useTranslation } from "react-i18next";

export function DecisionEvaluationDetails({ evaluation }: { evaluation: DecisionEvaluation | null | undefined }) {
	const { t } = useTranslation("common");
	if (!evaluation) return null;
	return (
		<details className="mt-2 min-w-0 basis-full text-[11px] text-muted-foreground">
			<summary className="cursor-pointer">{t("settings.decisionProvider.evaluationDetails")}</summary>
			<p className="mt-1">{t("settings.decisionProvider.probabilityHint")}</p>
			{evaluation.contextTruncated ? <p className="mt-1">{t("settings.decisionProvider.contextIncomplete")}</p> : null}
			<dl className="mt-2 space-y-1">
				{evaluation.scores.map((score) => (
					<div key={score.key} className="flex justify-between gap-3">
						<dt className="min-w-0 break-words">{t(`settings.decisionProvider.criteria.${score.key}`, { defaultValue: score.key })}</dt>
						<dd className="font-mono">{Math.round(score.probability * 100)}%</dd>
					</div>
				))}
			</dl>
		</details>
	);
}
