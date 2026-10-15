/** The projects the home can be scoped to, and the one it is scoped to now. */
import { projectGroups, type ProjectGroup, type WorkspaceScope } from "@dcc/mobile-core";
import { useMemo } from "react";
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
