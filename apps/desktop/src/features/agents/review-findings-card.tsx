import { CornerDownLeft } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import { dispatchFindingsToAuthor } from "./finding-to-author-command";
import { dispatchOpenFinding } from "./open-finding-command";
import type { ReviewFinding, ReviewSeverity } from "./review-findings";
import { useAgents } from "./use-agents";
import { useReviewerFindings } from "./use-reviewer-findings";

const SEVERITY_ORDER: Record<ReviewSeverity, number> = { critical: 0, major: 1, minor: 2 };
const SEVERITY_DOT: Record<ReviewSeverity, string> = {
	critical: "bg-red-500",
	major: "bg-amber-500",
	minor: "bg-muted-foreground/45",
};

function sameFindings(left: ReviewFinding[], right: ReviewFinding[]) {
	return JSON.stringify(left) === JSON.stringify(right);
}

/**
 * The reviewer's findings as a list in its own message: the place to read
 * them, open each one on the diff and hand them to the author agent. Once the
 * task changed after the review the list stays readable but stops acting, so
 * nobody sends a finding about lines that have moved.
 */
export function ReviewFindingsCard({
	sessionId,
	findings,
	scope = "local",
}: {
	sessionId: string | null | undefined;
	findings: ReviewFinding[];
	scope?: string;
}) {
	const { t } = useTranslation("common");
	const { agents } = useAgents(scope);
	const workspaceId =
		agents.flatMap((agent) => agent.sessions).find((session) => session.sessionId === sessionId)
			?.workspaceId ?? null;
	const current = sameFindings(useReviewerFindings(workspaceId, scope), findings);
	const ordered = [...findings].sort(
		(left, right) => SEVERITY_ORDER[left.severity] - SEVERITY_ORDER[right.severity],
	);
	const forAuthor = (finding: ReviewFinding) => ({
		path: finding.path,
		startLine: finding.startLine,
		endLine: finding.endLine,
		title: finding.title,
		detail: finding.detail,
		snippet: "",
	});

	return (
		<section
			aria-label={t("agents.review.cardTitle", { count: findings.length })}
			className="mt-3 max-w-2xl overflow-hidden rounded-[14px] border border-border/70 bg-card"
		>
			<header className="flex items-center gap-3 border-b border-border/60 px-3.5 py-2">
				<div className="min-w-0 flex-1">
					<h3 className="text-[13px] font-semibold text-foreground">
						{t("agents.review.cardTitle", { count: findings.length })}
					</h3>
					{!current && (
						<p className="mt-0.5 text-[11px] text-muted-foreground">{t("agents.review.stale")}</p>
					)}
				</div>
				{findings.length > 1 && (
					<Button
						type="button"
						size="sm"
						variant="outline"
						disabled={!current}
						onClick={() => dispatchFindingsToAuthor(workspaceId, ordered.map(forAuthor))}
					>
						<CornerDownLeft className="size-3.5" />
						<span>{t("agents.review.sendAll")}</span>
					</Button>
				)}
			</header>
			<ul className="divide-y divide-border/50">
				{ordered.map((finding) => {
					const location =
						finding.startLine === finding.endLine
							? `${finding.path}:${finding.startLine}`
							: `${finding.path}:${finding.startLine}–${finding.endLine}`;
					return (
						<li
							key={`${finding.path}:${finding.startLine}:${finding.title}`}
							className="flex items-start gap-1 pr-2"
						>
							<button
								type="button"
								disabled={!current || !workspaceId}
								title={finding.detail || finding.title}
								onClick={() =>
									workspaceId &&
									dispatchOpenFinding({ workspaceId, path: finding.path, line: finding.endLine })
								}
								className="flex min-w-0 flex-1 cursor-pointer items-start gap-2.5 px-3.5 py-2.5 text-left transition-colors hover:bg-accent/50 disabled:cursor-default disabled:hover:bg-transparent"
							>
								<span
									className={cn("mt-[5px] size-2 shrink-0 rounded-full", SEVERITY_DOT[finding.severity])}
									aria-hidden
								/>
								<span className="min-w-0 flex-1">
									<span className="block text-[13px] leading-snug text-foreground">
										{finding.title}
									</span>
									<span className="mt-1 block truncate font-mono text-[11px] text-muted-foreground">
										{t(`agents.review.severity.${finding.severity}`)} · {location}
									</span>
								</span>
							</button>
							<Tooltip>
								<TooltipTrigger asChild>
									<Button
										type="button"
										size="icon-sm"
										variant="ghost"
										className="mt-1.5 shrink-0"
										disabled={!current}
										aria-label={t("agents.review.sendOne")}
										onClick={() => dispatchFindingsToAuthor(workspaceId, [forAuthor(finding)])}
									>
										<CornerDownLeft className="size-3.5" />
									</Button>
								</TooltipTrigger>
								<TooltipContent side="left">{t("agents.review.sendOne")}</TooltipContent>
							</Tooltip>
						</li>
					);
				})}
			</ul>
		</section>
	);
}
