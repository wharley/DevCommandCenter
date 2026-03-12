/**
 * Worktree Service - Cria e gerencia worktrees Git por missão
 *
 * Permite N missões em paralelo no mesmo projeto, cada uma em sua worktree.
 * Ver: docs/WORKTREE_POLICY.md e docs/PLAN_REMAINING_IMPLEMENTATION.md
 */

import { exec } from "node:child_process";
import { promisify } from "node:util";
import * as fs from "node:fs";
import * as path from "node:path";
import { WORKTREE_SUBDIR } from "./worktree-policy";

const execAsync = promisify(exec);

/** Branch name safe for Git (no spaces, no slash) */
function safeBranchName(missionId: string): string {
  return `dcc-mission-${missionId.replace(/[^a-zA-Z0-9-_]/g, "-").slice(0, 80)}`;
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
 * Cria um worktree para uma missão a partir do repositório principal.
 * Path: <projectPath>/.dcc-worktrees/<branch>
 * Branch: dcc-mission-<missionId>
 */
export async function createWorktreeForMission(
  projectPath: string,
  missionId: string
): Promise<{ success: true; data: CreateWorktreeResult } | { success: false; error: string }> {
  const resolvedProject = path.resolve(projectPath);
  const branch = safeBranchName(missionId);
  const worktreeDir = path.join(resolvedProject, WORKTREE_SUBDIR, branch);

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

    await fs.promises.mkdir(path.join(resolvedProject, WORKTREE_SUBDIR), {
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
