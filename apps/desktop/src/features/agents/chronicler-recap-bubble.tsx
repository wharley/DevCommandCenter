import { useQuery } from "@tanstack/react-query";
import { X } from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { agentsActivityRecap } from "@/lib/agents-api";
import { useThoughtTyping } from "./agent-review-offer";
import { dismissDailyRecapBubble, type RecapWindow } from "./daily-recap";
import "./agents.css";

/**
 * What the chronicler says under its sidebar row, once per recap: the
 * recap's numbers. Null while they load, and when there is nothing to report.
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

/**
 * The chronicler thinking out loud under its sidebar row: dots, then the
 * recap's numbers written out, then a button to open it and one to close the
 * bubble. Closing only hides the bubble; the result-ready badge stays until
 * the recap is opened.
 */
export function ChroniclerRecapBubble({
	name,
	text,
	onOpen,
}: {
	name: string;
	text: string;
	onOpen: () => void;
}) {
	const { t } = useTranslation("common");
	return (
		// A count that arrives late (blockers load after the recap) extends
		// the text, and the typing carries on from where it was.
		<ReporterThought
			name={name}
			text={text}
			dismissLabel={t("agents.offer.dismiss")}
			onDismiss={dismissDailyRecapBubble}
		>
			<Button type="button" size="sm" onClick={onOpen}>
				<span>{t("agents.recap.bubble.open")}</span>
			</Button>
		</ReporterThought>
	);
}

/**
 * A thought rising to the reporter's mascot in the row above: dots, the text
 * written out, then a close button and the given actions.
 */
export function ReporterThought({
	name,
	text,
	dismissLabel,
	dismissDisabled = false,
	onDismiss,
	children,
}: {
	name: string;
	text: string;
	dismissLabel: string;
	dismissDisabled?: boolean;
	onDismiss: () => void;
	children: ReactNode;
}) {
	const { typed, thinking, ready } = useThoughtTyping(text);
	return (
		<div className="dcc-agent-offer mb-1 ml-2 mr-1">
			{/* The thought rises to the mascot in the row above. */}
			<div className="flex flex-col items-start gap-0.5 pl-2.5" aria-hidden>
				<span className="size-1.5 rounded-full border border-border/70 bg-card" />
				<span className="ml-1 size-2.5 rounded-full border border-border/70 bg-card" />
			</div>
			<div className="mt-0.5 rounded-[14px] border border-border/70 bg-card px-3 py-2 shadow-sm">
				<p className="sr-only" aria-live="polite">
					{name}: {text}
				</p>
				{thinking ? (
					<span className="dcc-agent-thinking flex items-center gap-1 py-1.5" aria-hidden>
						<span />
						<span />
						<span />
					</span>
				) : (
					<p className="text-[12px] leading-relaxed text-foreground" aria-hidden>
						{text.slice(0, typed)}
						{!ready && <span className="dcc-agent-caret" />}
					</p>
				)}
				{ready && (
					<span className="dcc-agent-offer-actions mt-1.5 flex flex-wrap items-center justify-end gap-1">
						<Button
							type="button"
							variant="ghost"
							size="icon-sm"
							aria-label={dismissLabel}
							disabled={dismissDisabled}
							onClick={onDismiss}
						>
							<X className="size-3.5" />
						</Button>
						{children}
					</span>
				)}
			</div>
		</div>
	);
}
