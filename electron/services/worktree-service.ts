/**
 * Worktree Service - Cria e gerencia worktrees Git por missão
 *
 * Permite N missões em paralelo no mesmo projeto, cada uma em sua worktree.
 * Worktrees ficam dentro do projeto (estilo dmux): project/.dcc/worktrees/<branch>
 * Ver: docs/WORKTREE_POLICY.md e docs/PLAN_REMAINING_IMPLEMENTATION.md
 */

import { exec, execFile } from "node:child_process";
import { promisify } from "node:util";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";

import { GitService } from "./git-service";
import type { GitStatus } from "./types";

const execAsync = promisify(exec);
const execFileAsync = promisify(execFile);

const gitEnv = { ...process.env, GIT_TERMINAL_PROMPT: "0" };

/** Mensagem quando o repositório principal tem alterações locais (merge/checkout/apply). */
function formatMainRepoDirtyError(status: GitStatus): string {
  const all = [
    ...new Set([
      ...status.staged,
      ...status.unstaged,
      ...status.untracked,
    ]),
  ];
  const preview = all.slice(0, 12);
  const list =
    preview.length > 0
      ? ` Ex.: ${preview.join(", ")}${all.length > 12 ? "…" : "."}`
      : "";
  return (
    "O repositório principal tem alterações não guardadas (commit, stash ou descarte antes de continuar)." +
    list
  );
}

function looksLikeDirtyTreeGitError(message: string): boolean {
  return (
    /local changes would be overwritten/i.test(message) ||
    /Please commit your changes or stash/i.test(message) ||
    /Your local changes to the following files would be overwritten/i.test(
      message,
    )
  );
}

function translateMainRepoGitError(raw: string): string {
  const t = raw.trim();
  if (looksLikeDirtyTreeGitError(t)) {
    return (
      "O repositório principal tem alterações que impedem mudar de branch ou aplicar o patch. " +
      "Faça commit, stash ou descarte no repositório principal e tente de novo."
    );
  }
  return raw;
}

/**
 * Aplica alterações copiando paths do worktree para o repo principal (fallback quando `git apply` falha).
 * Remoções: se o ficheiro não existe no worktree, remove no destino se existir.
 */
async function copyWorktreePathsToProject(
  worktreeRoot: string,
  projectRoot: string,
  relativePaths: string[],
): Promise<{ success: boolean; error?: string }> {
  for (const rel of relativePaths) {
    const src = path.join(worktreeRoot, rel);
    const dst = path.join(projectRoot, rel);
    try {
      if (fs.existsSync(src)) {
        const stat = await fs.promises.stat(src);
        if (stat.isDirectory()) {
          continue;
        }
        await fs.promises.mkdir(path.dirname(dst), { recursive: true });
        await fs.promises.copyFile(src, dst);
      } else if (fs.existsSync(dst)) {
        await fs.promises.unlink(dst);
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      return { success: false, error: msg };
    }
  }
  return { success: true };
}

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
 * @param baseBranch - Branch de origem (ref) para criar o worktree. Se não informado, usa HEAD.
 */
export async function createWorktreeForMission(
  projectPath: string,
  missionId: string,
  missionTitle?: string | null,
  baseBranch?: string | null,
): Promise<{ success: true; data: CreateWorktreeResult } | { success: false; error: string }> {
  const resolvedProject = path.resolve(projectPath);
  const branch = safeBranchName(missionId, missionTitle);
  const worktreeDir = getProjectWorktreeDir(resolvedProject, branch);
  const fromRef = (baseBranch && baseBranch.trim()) ? baseBranch.trim() : "HEAD";

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
      `git worktree add "${worktreeDir}" -b "${branch}" "${fromRef}"`,
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

function safeCombBranchName(combId: string, combName?: string | null): string {
  const slug = slugifyForBranch(combName).slice(0, 36);
  const shortId =
    combId.replace(/[^a-zA-Z0-9]/g, "").slice(0, 8).toLowerCase() || "unknown";
  return `dcc-comb-${slug}-${shortId}`;
}

/**
 * Creates a worktree for a Comb.
 * Path: <projectPath>/.dcc/worktrees/<branch>
 */
export async function createWorktreeForComb(
  projectPath: string,
  combId: string,
  combName?: string | null,
  baseBranch?: string | null,
): Promise<{ success: true; data: CreateWorktreeResult } | { success: false; error: string }> {
  const resolvedProject = path.resolve(projectPath);
  const branch = safeCombBranchName(combId, combName);
  const worktreeDir = getProjectWorktreeDir(resolvedProject, branch);
  const fromRef = (baseBranch && baseBranch.trim()) ? baseBranch.trim() : "HEAD";

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

    await fs.promises.mkdir(path.dirname(worktreeDir), { recursive: true });

    if (fs.existsSync(worktreeDir)) {
      const exists = await branchExistsInRepo(resolvedProject, branch);
      if (exists) {
        return {
          success: true,
          data: { worktreePath: worktreeDir, worktreeBranch: branch },
        };
      }
      await fs.promises.rm(worktreeDir, { recursive: true, force: true });
    }

    await execAsync(
      `git worktree add "${worktreeDir}" -b "${branch}" "${fromRef}"`,
      { cwd: resolvedProject }
    );

    return {
      success: true,
      data: { worktreePath: worktreeDir, worktreeBranch: branch },
    };
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : String(err);
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
    const mainGit = new GitService(resolvedProject);
    const mainStatus = await mainGit.getStatus();
    if (mainStatus.isDirty) {
      return { success: false, error: formatMainRepoDirtyError(mainStatus) };
    }

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
    return { success: false, error: translateMainRepoGitError(message) };
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

export interface ApplyMissionPatchOptions {
  /** Arquivos a incluir no patch (paths relativos ao repo). Se vazio, inclui todos. */
  includeFiles?: string[];
  commit?: boolean;
  message?: string;
}

/**
 * Gera patch do worktree (git diff HEAD) e aplica no repo principal na targetBranch.
 * Opcionalmente faz commit. Usa diff único --binary para tracked + untracked; se `git apply`
 * falhar, copia ficheiros do worktree para o projeto principal.
 */
export async function applyMissionPatch(
  projectPath: string,
  worktreePath: string,
  targetBranch: string,
  options: ApplyMissionPatchOptions = {}
): Promise<WorktreeServiceResult & { applyFailed?: boolean }> {
  const resolvedProject = path.resolve(projectPath);
  const resolvedWorktree = path.resolve(worktreePath);
  const { includeFiles = [], commit = false, message = "Apply mission patch" } = options;

  try {
    const git = new GitService(resolvedWorktree);
    const branchState = await git.getBranchState();
    const untrackedSet = new Set(branchState.untracked ?? []);
    const allChanged = branchState.changedFiles ?? [];
    const pathsToInclude =
      includeFiles.length > 0 ? includeFiles : allChanged;

    const trackedPaths = pathsToInclude.filter((p) => !untrackedSet.has(p));
    const untrackedPaths = pathsToInclude.filter((p) => untrackedSet.has(p));

    const parts: string[] = [];
    const trackedChunk = await git.getTrackedDiffBinaryVsHead(trackedPaths);
    if (trackedChunk.trim()) {
      parts.push(trackedChunk.trimEnd());
    }
    for (const filePath of untrackedPaths) {
      const chunk = await git.getFileDiffUntracked(filePath);
      if (chunk.trim()) {
        parts.push(chunk.trimEnd());
      }
    }

    let patchContent = parts.join("\n");
    if (patchContent.length > 0 && !patchContent.endsWith("\n")) {
      patchContent += "\n";
    }
    if (!patchContent.trim()) {
      return { success: false, error: "Nenhuma alteração para aplicar." };
    }

    const tmpFile = path.join(
      os.tmpdir(),
      `dcc-apply-${Date.now()}-${Math.random().toString(36).slice(2)}.patch`
    );
    await fs.promises.writeFile(tmpFile, patchContent, "utf-8");

    const mainRepoGit = new GitService(resolvedProject);
    const mainStatus = await mainRepoGit.getStatus();
    if (mainStatus.isDirty) {
      await fs.promises.unlink(tmpFile).catch(() => {});
      return { success: false, error: formatMainRepoDirtyError(mainStatus) };
    }

    try {
      await execFileAsync("git", ["checkout", targetBranch], {
        cwd: resolvedProject,
        env: gitEnv,
      });
    } catch (checkoutErr: unknown) {
      await fs.promises.unlink(tmpFile).catch(() => {});
      const raw =
        checkoutErr instanceof Error ? checkoutErr.message : String(checkoutErr);
      return { success: false, error: translateMainRepoGitError(raw) };
    }

    try {
      await execFileAsync("git", ["apply", "--check", tmpFile], {
        cwd: resolvedProject,
        env: gitEnv,
      });
      await execFileAsync("git", ["apply", tmpFile], {
        cwd: resolvedProject,
        env: gitEnv,
      });
    } catch (applyErr: unknown) {
      const errMsg = applyErr instanceof Error ? applyErr.message : String(applyErr);
      const stderr =
        applyErr &&
        typeof applyErr === "object" &&
        "stderr" in applyErr
          ? String((applyErr as { stderr?: string }).stderr ?? "")
          : "";
      const combinedErr = (stderr.trim() || errMsg).trim();

      const copyResult = await copyWorktreePathsToProject(
        resolvedWorktree,
        resolvedProject,
        pathsToInclude,
      );
      if (!copyResult.success) {
        return {
          success: false,
          error: `${combinedErr} (fallback cópia: ${copyResult.error})`,
          applyFailed: true,
        };
      }
      console.log(
        "[applyMissionPatch] git apply failed; applied changes via file copy fallback",
        combinedErr.slice(0, 200),
      );
    } finally {
      await fs.promises.unlink(tmpFile).catch(() => {});
    }

    if (commit) {
      const { stdout: statusOut } = await execAsync("git status --porcelain", {
        cwd: resolvedProject,
      });
      const files = statusOut
        .trim()
        .split(/\n/)
        .filter(Boolean)
        .map((line) => line.slice(3).trim());
      if (files.length > 0) {
        for (const f of files) {
          await execAsync(`git add "${f.replace(/"/g, '\\"')}"`, {
            cwd: resolvedProject,
          });
        }
        const escapedMsg = message.replace(/"/g, '\\"');
        await execAsync(`git commit -m "${escapedMsg}"`, { cwd: resolvedProject });
      }
    }

    return { success: true };
  } catch (err: unknown) {
    const message =
      err instanceof Error ? err.message : String(err);
    return { success: false, error: translateMainRepoGitError(message) };
  }
}
