import { ArrowRight, GitBranch } from "lucide-react";
import { useTranslation } from "react-i18next";

/** Wrapping branch names shared by review surfaces; preserve the complete refs. */
export function ReviewBranchContext({
	head,
	base,
}: {
	head: string | null | undefined;
	base: string | null | undefined;
}) {
	const { t } = useTranslation("common");
	return (
		<div className="dcc-review-branches">
			<GitBranch size={13} aria-hidden />
			<span>
				<small>{t("review.branch")}</small>
				<code>{head || "—"}</code>
			</span>
			{base && (
				<>
					<ArrowRight size={12} aria-hidden />
					<span>
						<small>{t("review.base")}</small>
						<code>{base}</code>
					</span>
				</>
			)}
		</div>
	);
}
