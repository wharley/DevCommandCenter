import { useQuery } from "@tanstack/react-query";
import { Loader2 } from "lucide-react";
import { useEffect, useMemo, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { agentsActivityRecap } from "@/lib/agents-api";
import { cn } from "@/lib/utils";
import type { WorkspaceSummary } from "@/features/workspaces/types";
import {
	attentionWorkspaceItems,
	useWorkspaceAgentActivities,
} from "@/features/workspaces/use-workspace-agent-states";
import { blockersByWorkspace, useWorkspaceBlockers } from "@/features/workspaces/use-workspace-blockers";
import {
	isWorkspaceResultUnread,
	useSeenWorkspaceResults,
} from "@/features/workspaces/workspace-seen-results";
import { workspaceRailDisplayTitle } from "@/features/workspaces/workspace-rail-shared";
import { markDailyRecapSeen, RECAP_BLOCKING_REASONS, useDailyRecap } from "./daily-recap";

const SECTION_TITLE =
	"mb-2 text-[11px] font-semibold uppercase tracking-[0.08em] text-muted-foreground";


type RecapRow = {
	workspaceId: string;
	title: string;
	detail: string;
	tone?: "attention";
};

/**
 * The chronicler's recap: what blocks the person now, then what moved since
 * the previous recap. Built from what DCC recorded, without a model. Seeing
 * this page is what marks the recap as read.
 */
export function DailyRecapCard({
	workspaces,
	projectLabels,
	scope,
	onOpenWorkspace,
}: {
	workspaces: WorkspaceSummary[];
	projectLabels: Record<string, string>;
	scope: string;
	onOpenWorkspace: (workspaceId: string) => void;
}) {
	const { t, i18n } = useTranslation("common");
	const recapWindow = useDailyRecap().current;
	useEffect(() => {
		markDailyRecapSeen();
	}, [recapWindow?.to]);

	const recapQuery = useQuery({
		queryKey: ["agentsActivityRecap", recapWindow?.from ?? null, recapWindow?.to ?? null],
		queryFn: () => agentsActivityRecap(recapWindow!.from, recapWindow!.to),
		enabled: Boolean(recapWindow),
		staleTime: Number.POSITIVE_INFINITY,
	});

	const activities = useWorkspaceAgentActivities(workspaces, { scope });
	const seenResults = useSeenWorkspaceResults();
	const blockersQuery = useWorkspaceBlockers(scope === "local");
	const blocked = useMemo(
		() =>
			attentionWorkspaceItems(
				workspaces,
				activities,
				(workspaceId, completedAt) =>
					isWorkspaceResultUnread(completedAt, seenResults.seen[workspaceId], seenResults.baseline),
				blockersByWorkspace(blockersQuery.data ?? []),
			).filter((item) => RECAP_BLOCKING_REASONS.has(item.reason)),
		[activities, blockersQuery.data, seenResults, workspaces],
	);

	const byId = useMemo(
		() => new Map(workspaces.map((workspace) => [workspace.id, workspace])),
		[workspaces],
	);
	const timeFormat = new Intl.DateTimeFormat(i18n.language, {
		weekday: "short",
		hour: "2-digit",
		minute: "2-digit",
	});
	const fromLabel = recapWindow ? timeFormat.format(new Date(recapWindow.from)) : "";
	const titleOf = (workspaceId: string) => {
		const workspace = byId.get(workspaceId);
		return workspace ? workspaceRailDisplayTitle(workspace) : t("agents.recap.removedTask");
	};
	const projectOf = (workspaceId: string) => {
		const projectId = byId.get(workspaceId)?.projectId;
		return projectId ? projectLabels[projectId] : undefined;
	};

	const blockedRows: RecapRow[] = blocked.map(({ workspace, reason, count, returnedAt }) => ({
		workspaceId: workspace.id,
		title: workspaceRailDisplayTitle(workspace),
		detail: [
			t(`sidebar.attention.reason.${reason}`, { count: count ?? 0, pr: count ?? "" }),
			returnedAt ? t("agents.recap.since", { time: timeFormat.format(new Date(returnedAt)) }) : null,
			projectOf(workspace.id),
		]
			.filter(Boolean)
			.join(" · "),
		tone: "attention",
	}));
	const recap = recapQuery.data;
	const movedRows: RecapRow[] = (recap?.tasks ?? []).map((task) => ({
		workspaceId: task.workspaceId,
		title: titleOf(task.workspaceId),
		detail: [
			task.completedTurns > 0 ? t("agents.recap.turns", { count: task.completedTurns }) : null,
			task.abortedTurns > 0 ? t("agents.recap.aborted", { count: task.abortedTurns }) : null,
			projectOf(task.workspaceId),
			timeFormat.format(new Date(task.lastActivityAt)),
		]
			.filter(Boolean)
			.join(" · "),
	}));
	const listRows = (ids: string[]): RecapRow[] =>
		ids.map((workspaceId) => ({
			workspaceId,
			title: titleOf(workspaceId),
			detail: projectOf(workspaceId) ?? "",
		}));
	const completedRows = listRows(recap?.completedWorkspaceIds ?? []);
	const createdRows = listRows(recap?.createdWorkspaceIds ?? []);
	const nothingMoved =
		recap && movedRows.length === 0 && completedRows.length === 0 && createdRows.length === 0;

	return (
		<div className="flex flex-col gap-6">
			<header>
				<h2 className="text-[15px] font-semibold">{t("agents.recap.title", { from: fromLabel })}</h2>
				<p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
					{t("agents.recap.howItWorks")}
				</p>
			</header>

			{blockedRows.length > 0 && (
				<RecapSection title={t("agents.recap.blocked")}>
					<RecapList rows={blockedRows} onOpen={onOpenWorkspace} />
				</RecapSection>
			)}

			{recapQuery.isPending ? (
				<p className="flex items-center gap-2 text-[13px] text-muted-foreground">
					<Loader2 className="size-4 animate-spin" />
					{t("agents.recap.loading")}
				</p>
			) : recapQuery.isError ? (
				<p className="text-[13px] text-destructive">{t("agents.recap.failed")}</p>
			) : nothingMoved ? (
				<p className="rounded-[18px] border border-dashed border-border/70 px-5 py-6 text-[13px] text-muted-foreground">
					{t("agents.recap.empty", { from: fromLabel })}
				</p>
			) : (
				<>
					{movedRows.length > 0 && (
						<RecapSection title={t("agents.recap.moved")}>
							<RecapList rows={movedRows} onOpen={onOpenWorkspace} />
						</RecapSection>
					)}
					{completedRows.length > 0 && (
						<RecapSection title={t("agents.recap.completed")}>
							<RecapList rows={completedRows} onOpen={onOpenWorkspace} />
						</RecapSection>
					)}
					{createdRows.length > 0 && (
						<RecapSection title={t("agents.recap.created")}>
							<RecapList rows={createdRows} onOpen={onOpenWorkspace} />
						</RecapSection>
					)}
				</>
			)}
		</div>
	);
}

function RecapSection({ title, children }: { title: string; children: ReactNode }) {
	return (
		<section>
			<h3 className={SECTION_TITLE}>{title}</h3>
			{children}
		</section>
	);
}

function RecapList({ rows, onOpen }: { rows: RecapRow[]; onOpen: (workspaceId: string) => void }) {
	return (
		<ul className="divide-y divide-border/60 overflow-hidden rounded-[18px] border border-border/70 bg-card">
			{rows.map((row) => (
				<li key={row.workspaceId}>
					<button
						type="button"
						onClick={() => onOpen(row.workspaceId)}
						className="flex w-full cursor-pointer flex-col px-4 py-3 text-left transition-colors hover:bg-accent/50"
					>
						<span className="truncate text-[13px] font-medium text-foreground">{row.title}</span>
						{row.detail && (
							<span
								className={cn(
									"truncate text-[11px]",
									row.tone === "attention"
										? "text-amber-700 dark:text-amber-300"
										: "text-muted-foreground",
								)}
							>
								{row.detail}
							</span>
						)}
					</button>
				</li>
			))}
		</ul>
	);
}
