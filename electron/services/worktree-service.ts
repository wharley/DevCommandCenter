/**
 * Worktree Service - Cria e gerencia worktrees Git por missão
 *
 * Permite N missões em paralelo no mesmo projeto, cada uma em sua worktree.
 * Worktrees ficam dentro do projeto (estilo dmux): project/.dcc/worktrees/<branch>
 * Ver: docs/WORKTREE_POLICY.md e docs/PLAN_REMAINING_IMPLEMENTATION.md
 */

import { exec } from "node:child_process";
import { promisify } from "node:util";
import * as fs from "node:fs";
import * as path from "node:path";

const execAsync = promisify(exec);

/** Diretório relativo ao projeto onde as worktrees são criadas (estilo dmux) */
export const WORKTREE_RELATIVE_DIR = ".dcc/worktrees";

function slugifyForBranch(input?: string | null): string {
  if (!input) return "mission";
  const normalized = input
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return normalized || "mission";
}

/** Branch name safe for Git (no spaces, no slash) */
function safeBranchName(missionId: string, missionTitle?: string | null): string {
  const slug = slugifyForBranch(missionTitle).slice(0, 36);
  const shortId =
    missionId.replace(/[^a-zA-Z0-9]/g, "").slice(0, 8).toLowerCase() ||
    "unknown";
  return `dcc-mission-${slug}-${shortId}`;
}

export interface CreateWorktreeResult {
  worktreePath: string;
  worktreeBranch: string;
}

export interface WorktreeServiceResult {
  success: boolean;
  error?: string;
}

/**
 * Retorna o path da worktree dentro do projeto (project/.dcc/worktrees/<branch>).
 * Igual ao dmux: worktrees ficam no próprio repo, não em pasta global.
 */
function getProjectWorktreeDir(projectPath: string, branch: string): string {
  const resolvedProject = path.resolve(projectPath);
  return path.join(resolvedProject, WORKTREE_RELATIVE_DIR, branch);
}

/**
 * Cria um worktree para uma missão a partir do repositório principal.
 * Path: <projectPath>/.dcc/worktrees/<branch> (dentro do projeto, como no dmux)
 * Branch: dcc-mission-<mission-slug>-<missionIdShort>
 */
export async function createWorktreeForMission(
  projectPath: string,
  missionId: string,
  missionTitle?: string | null,
): Promise<{ success: true; data: CreateWorktreeResult } | { success: false; error: string }> {
  const resolvedProject = path.resolve(projectPath);
  const branch = safeBranchName(missionId, missionTitle);
  const worktreeDir = getProjectWorktreeDir(resolvedProject, branch);

  try {
    const stat = await fs.promises.stat(resolvedProject);
    if (!stat.isDirectory()) {
      return { success: false, error: "Project path is not a directory" };
    }

    const gitDir = path.join(resolvedProject, ".git");
    const gitExists =
      (await fs.promises.stat(gitDir).then(() => true).catch(() => false)) ||
      (await fs.promises.stat(gitDir + "/file").then(() => true).catch(() => false));
    if (!gitExists) {
      return { success: false, error: "Project is not a Git repository" };
    }

    await fs.promises.mkdir(path.dirname(worktreeDir), {
      recursive: true,
    });

    if (fs.existsSync(worktreeDir)) {
      const branchExists = await branchExistsInRepo(resolvedProject, branch);
      if (branchExists) {
        return {
          success: true,
          data: {
            worktreePath: worktreeDir,
            worktreeBranch: branch,
          },
        };
      }
      await fs.promises.rm(worktreeDir, { recursive: true, force: true });
    }

    await execAsync(
      `git worktree add "${worktreeDir}" -b "${branch}"`,
      { cwd: resolvedProject }
    );

    return {
      success: true,
      data: {
        worktreePath: worktreeDir,
        worktreeBranch: branch,
      },
    };
  } catch (err: unknown) {
    const message =
      err instanceof Error ? err.message : String(err);
    return { success: false, error: message };
  }
}

async function branchExistsInRepo(
  projectPath: string,
  branch: string
): Promise<boolean> {
  try {
    await execAsync(`git rev-parse --verify "${branch}"`, {
      cwd: projectPath,
    });
    return true;
  } catch {
    return false;
  }
}

/**
 * Remove um worktree e opcionalmente o branch.
 */
export async function removeWorktree(
  projectPath: string,
  worktreePath: string,
  options?: { force?: boolean; deleteBranch?: boolean }
): Promise<WorktreeServiceResult> {
  const resolvedProject = path.resolve(projectPath);
  const resolvedWorktree = path.resolve(worktreePath);

  try {
    const force = options?.force ?? false;
    await execAsync(
      `git worktree remove ${force ? "--force " : ""}"${resolvedWorktree}"`,
      { cwd: resolvedProject }
    );

    if (options?.deleteBranch) {
      const branch = path.basename(resolvedWorktree);
      try {
        await execAsync(`git branch -D "${branch}"`, {
          cwd: resolvedProject,
        });
      } catch {
        // Branch may already be deleted or current
      }
    }
    return { success: true };
  } catch (err: unknown) {
    const message =
      err instanceof Error ? err.message : String(err);
    return { success: false, error: message };
  }
}

/**
 * Faz merge do branch do worktree no branch principal e remove o worktree.
 */
export async function mergeWorktreeIntoMain(
  projectPath: string,
  worktreeBranch: string,
  worktreePath: string,
  mainBranch: string = "main"
): Promise<WorktreeServiceResult> {
  const resolvedProject = path.resolve(projectPath);

  try {
    await execAsync(`git checkout "${mainBranch}"`, { cwd: resolvedProject });
    await execAsync(`git merge "${worktreeBranch}" --no-edit`, {
      cwd: resolvedProject,
    });
    await removeWorktree(resolvedProject, worktreePath, {
      force: false,
      deleteBranch: true,
    });
    return { success: true };
  } catch (err: unknown) {
    const message =
      err instanceof Error ? err.message : String(err);
    return { success: false, error: message };
  }
}

/**
 * Descarta o worktree (remove sem merge) e deleta o branch.
 */
export async function discardWorktree(
  projectPath: string,
  worktreePath: string,
  _worktreeBranch: string
): Promise<WorktreeServiceResult> {
  return removeWorktree(projectPath, worktreePath, {
    force: true,
    deleteBranch: true,
  });
}
