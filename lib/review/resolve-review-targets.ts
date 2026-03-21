import type { Comb, Project, ReviewTarget } from "@/lib/database/types";

/** Target resolvido com paths de Git no disco. */
export interface ResolvedReviewTarget {
  id: string;
  label: string;
  projectId: string;
  /** Raiz do repositório (repo principal). */
  mainProjectPath: string;
  /** Onde rodar git status/diff (worktree da missão ou checkout local). */
  worktreePath: string;
  /** Diffs vêm da worktree do comb (`.dcc/worktrees/...`). */
  isPrimaryCombWorktree: boolean;
}

function targetKey(t: ReviewTarget): string {
  return `${t.projectId}::${t.sourceCombId ?? ""}`;
}

/**
 * Junta o target primário (projeto da Missão + worktree do comb) com extras persistidos.
 */
export function mergeReviewTargets(comb: Comb, primaryProjectName: string): ReviewTarget[] {
  const primary: ReviewTarget = {
    id: "primary",
    label: primaryProjectName,
    projectId: comb.projectId,
    sourceCombId: comb.id,
  };
  const extra = comb.reviewTargets ?? [];
  const seen = new Set<string>();
  const out: ReviewTarget[] = [];
  seen.add(targetKey(primary));
  out.push(primary);
  for (const t of extra) {
    if (!t.projectId || !t.id) continue;
    const k = targetKey(t);
    if (seen.has(k)) continue;
    seen.add(k);
    out.push({
      ...t,
      label: t.label?.trim() || t.id,
    });
  }
  return out;
}

/**
 * Resolve paths reais por projeto. Targets sem `sourceCombId` alinhado ao comb usam `project.path` como worktree Git.
 */
export function resolveReviewTargets(
  comb: Comb,
  projects: Project[],
): ResolvedReviewTarget[] {
  const primaryProject = projects.find((p) => p.id === comb.projectId);
  const primaryName = primaryProject?.name ?? "Principal";
  const merged = mergeReviewTargets(comb, primaryName);
  const result: ResolvedReviewTarget[] = [];

  for (const t of merged) {
    const proj = projects.find((p) => p.id === t.projectId);
    const mainProjectPath = proj?.path?.trim() ?? "";
    if (!mainProjectPath) continue;

    const useCombWorktree =
      t.projectId === comb.projectId &&
      t.sourceCombId === comb.id &&
      Boolean(comb.worktreePath?.trim());

    const worktreePath = useCombWorktree ? comb.worktreePath!.trim() : mainProjectPath;

    result.push({
      id: t.id,
      label: t.label,
      projectId: t.projectId,
      mainProjectPath,
      worktreePath,
      isPrimaryCombWorktree: Boolean(useCombWorktree),
    });
  }

  if (
    result.length === 0 &&
    primaryProject?.path?.trim() &&
    comb.worktreePath?.trim()
  ) {
    result.push({
      id: "primary",
      label: primaryName,
      projectId: comb.projectId,
      mainProjectPath: primaryProject.path.trim(),
      worktreePath: comb.worktreePath.trim(),
      isPrimaryCombWorktree: true,
    });
  }

  return result;
}
