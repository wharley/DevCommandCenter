/** The projects the home can be scoped to, and the one it is scoped to now. */
import {
	projectDiffStat,
	projectGroups,
	workspaceDiffStat,
	type DeliverySummary,
	type ProjectGroup,
	type WorkspaceScope,
} from "@dcc/mobile-core";
import { useEffect, useMemo, useState } from "react";
import { useBackend } from "../backend/session";
import { useHomeData } from "./homeData";

export function useHomeScope(): {
	projects: ProjectGroup[] | null;
	/** Null when the home shows every project (or the scoped one is gone). */
	project: ProjectGroup | null;
	workspaces: WorkspaceScope;
	setScope(projectKey: string | null): void;
} {
	const { scan, combs, catalog, scopeKey, setScope } = useHomeData();
	const projects = useMemo(
		() => (combs ? projectGroups(catalog?.repositories ?? [], combs, scan?.sessions ?? []) : null),
		[catalog, combs, scan],
	);
	const project = useMemo(() => projects?.find((p) => p.key === scopeKey) ?? null, [projects, scopeKey]);
	const workspaces = useMemo(() => (project ? new Set(project.combs.map((c) => c.id)) : null), [project]);
	return { projects, project, workspaces, setScope };
}

/** At most this many worktrees are read for the line; the rest stay on the desktop. */
const MAX_WORKTREES = 8;

/**
 * What the scoped project changed, across its worktrees, for the line over
 * "Tarefas". Read again when an agent starts or stops in the project.
 */
export function useProjectDiff(project: ProjectGroup | null): ReturnType<typeof projectDiffStat> | null {
	const backend = useBackend();
	const { scan } = useHomeData();
	const [diff, setDiff] = useState<ReturnType<typeof projectDiffStat> | null>(null);
	const ids = project ? project.combs.slice(0, MAX_WORKTREES).map((c) => c.id) : [];
	const key = ids.join(",");
	// Running agents in the project: a change in that count means files may have changed.
	const live = useMemo(() => {
		if (!scan || !key) return 0;
		const inProject = new Set(key.split(","));
		return scan.sessions.filter(
			(s) => s.workspaceId && inProject.has(s.workspaceId) && scan.statuses.get(s.sessionId) === "running",
		).length;
	}, [scan, key]);
	useEffect(() => {
		setDiff(null);
		if (!key) return;
		let cancelled = false;
		void Promise.all(
			key.split(",").map((id) =>
				backend
					.request<DeliverySummary>("GET", `/api/v1/mobile/workspaces/${encodeURIComponent(id)}/delivery`)
					.then(workspaceDiffStat)
					.catch(() => null),
			),
		).then((stats) => {
			const read = stats.filter((stat): stat is NonNullable<typeof stat> => stat !== null);
			if (!cancelled && read.length > 0) setDiff(projectDiffStat(read));
		});
		return () => {
			cancelled = true;
		};
	}, [backend, key, live]);
	return diff;
}
