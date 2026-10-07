import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { agentsActivityRecap } from "@/lib/agents-api";
import type { RecapWindow } from "./daily-recap";

/**
 * What the reporter says above the composer, once per recap: the recap's
 * numbers. Null while they load, and when there is nothing to report.
 */
export function useRecapBubbleText(
	recapWindow: RecapWindow | null,
	/** Tasks blocked on the person right now, as the recap page counts them. */
	blockedCount: number,
): string | null {
	const { t, i18n } = useTranslation("common");
	// Same key as the recap page, so opening it reuses this answer.
	const recapQuery = useQuery({
		queryKey: ["agentsActivityRecap", recapWindow?.from ?? null, recapWindow?.to ?? null],
		queryFn: () => agentsActivityRecap(recapWindow!.from, recapWindow!.to),
		enabled: Boolean(recapWindow),
		staleTime: Number.POSITIVE_INFINITY,
	});
	const recap = recapQuery.data;
	if (!recapWindow || !recap) {
		return null;
	}
	const facts = [
		recap.tasks.length > 0 && t("agents.recap.bubble.moved", { count: recap.tasks.length }),
		recap.completedWorkspaceIds.length > 0 &&
			t("agents.recap.bubble.completed", { count: recap.completedWorkspaceIds.length }),
		recap.createdWorkspaceIds.length > 0 &&
			t("agents.recap.bubble.created", { count: recap.createdWorkspaceIds.length }),
		blockedCount > 0 && t("agents.recap.bubble.blocked", { count: blockedCount }),
	].filter((fact): fact is string => Boolean(fact));
	if (facts.length === 0) {
		return null;
	}
	const from = new Intl.DateTimeFormat(i18n.language, {
		weekday: "short",
		hour: "2-digit",
		minute: "2-digit",
	}).format(new Date(recapWindow.from));
	return t("agents.recap.bubble.lead", { from, facts: facts.join(", ") });
}
