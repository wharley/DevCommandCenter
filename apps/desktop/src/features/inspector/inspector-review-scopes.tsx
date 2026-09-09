import { Clock3, GitCompareArrows, Laptop } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { InspectorReviewScope } from "./inspector-changes-presentation";
const icons = {
	working: Laptop,
	"last-turn": Clock3,
	branch: GitCompareArrows,
};

export function InspectorReviewScopes({
	scopes,
	active,
	counts,
	onChange,
}: {
	scopes: InspectorReviewScope[];
	active: InspectorReviewScope;
	counts: Record<InspectorReviewScope, number | null>;
	onChange: (scope: InspectorReviewScope) => void;
}) {
	const { t } = useTranslation("common");
	return (
		<div className="dcc-review-scope-control">
			<div
				className="dcc-review-scopes"
				role="group"
				aria-label={t("inspector.changes.scopeLabel")}
			>
				{scopes.map((scope) => {
					const Icon = icons[scope];
					return (
						<button
							type="button"
							key={scope}
							aria-pressed={active === scope}
							aria-label={t(`inspector.changes.scopes.${scope}`)}
							title={t(`review.scopes.${scope}`)}
							onClick={() => onChange(scope)}
						>
							<Icon size={14} aria-hidden />
							<span>{t(`inspector.changes.scopes.${scope}`)}</span>
							<small>{counts[scope] ?? "—"}</small>
						</button>
					);
				})}
			</div>
			<p className="dcc-review-scope-hint">{t(`review.scopes.${active}`)}</p>
		</div>
	);
}
