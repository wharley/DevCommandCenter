/**
 * Git Service - Serviço para operações Git
 *
 * Fornece contexto do repositório para a IA e aplica mudanças
 */

import { exec, execFile } from "node:child_process";
import { promisify } from "node:util";
import * as fs from "node:fs";
import * as path from "node:path";
import * as os from "node:os";
import type {
  GitInfo,
  GitStatus,
  GitCommit,
  CodeSuggestion,
  ApplyChangesResult,
} from "./types";

const execAsync = promisify(exec);
const execFileAsync = promisify(execFile);

/** Detecta se o conteúdo parece diff unificado (linhas com +, -, ---, +++ ou contexto). */
function looksLikeUnifiedDiff(content: string): boolean {
  const trimmed = content.trim();
  if (!trimmed) return false;
  const lines = trimmed.split(/\r?\n/);
  return lines.some(
    (line) =>
      line.startsWith("+") ||
      line.startsWith("-") ||
      line.startsWith("--- ") ||
      line.startsWith("+++ "),
  );
}

export class GitService {
  private projectPath: string;

  constructor(projectPath: string) {
    this.projectPath = projectPath;
  }

  /**
   * Verifica se o diretório é um repositório Git
   */
  async isGitRepo(): Promise<boolean> {
    try {
      await execAsync("git rev-parse --is-inside-work-tree", {
        cwd: this.projectPath,
      });
      return true;
    } catch {
      return false;
    }
  }

  /**
   * Obtém informações completas do Git
   */
  async getGitInfo(): Promise<GitInfo | null> {
    const isRepo = await this.isGitRepo();
    if (!isRepo) return null;

    const [branch, remote, status, recentCommits] = await Promise.all([
      this.getCurrentBranch(),
      this.getRemoteUrl(),
      this.getStatus(),
      this.getRecentCommits(5),
    ]);

    return {
      branch,
      remote,
      status,
      recentCommits,
    };
  }

  /**
   * Obtém o branch atual
   */
  async getCurrentBranch(): Promise<string> {
    try {
      const { stdout } = await execAsync("git branch --show-current", {
        cwd: this.projectPath,
      });
      return stdout.trim() || "HEAD";
    } catch {
      return "unknown";
    }
  }

  /**
   * Obtém a URL do remote origin
   */
  async getRemoteUrl(): Promise<string | undefined> {
    try {
      const { stdout } = await execAsync("git remote get-url origin", {
        cwd: this.projectPath,
      });
      return stdout.trim();
    } catch {
      return undefined;
    }
  }

  /**
   * Obtém o status do repositório
   */
  async getStatus(): Promise<GitStatus> {
    const isRepo = await this.isGitRepo();
    if (!isRepo) {
      return {
        isRepo: false,
        isDirty: false,
        staged: [],
        unstaged: [],
        untracked: [],
      };
    }

    try {
      const { stdout } = await execAsync("git status --porcelain", {
        cwd: this.projectPath,
      });

      const staged: string[] = [];
      const unstaged: string[] = [];
      const untracked: string[] = [];

      const lines = stdout.trim().split("\n").filter(Boolean);
      for (const line of lines) {
        const indexStatus = line[0];
        const workTreeStatus = line[1];
        const filePath = line.slice(3);

        if (indexStatus === "?") {
          untracked.push(filePath);
        } else if (indexStatus !== " ") {
          staged.push(filePath);
        }

        if (workTreeStatus !== " " && workTreeStatus !== "?") {
          unstaged.push(filePath);
        }
      }

      return {
        isRepo: true,
        isDirty: lines.length > 0,
        staged,
        unstaged,
        untracked,
      };
    } catch {
      return {
        isRepo: true,
        isDirty: false,
        staged: [],
        unstaged: [],
        untracked: [],
      };
    }
  }

  /**
   * Obtém os commits mais recentes
   */
  async getRecentCommits(count: number = 10): Promise<GitCommit[]> {
    try {
      const { stdout } = await execAsync(
        `git log -${count} --format="%H|%s|%an|%aI"`,
        { cwd: this.projectPath },
      );

      return stdout
        .trim()
        .split("\n")
        .filter(Boolean)
        .map((line) => {
          const [hash, message, author, date] = line.split("|");
          return {
            hash,
            message,
            author,
            date: new Date(date),
          };
        });
    } catch {
      return [];
    }
  }

  /**
   * Obtém o diff de um arquivo específico
   */
  async getFileDiff(filePath: string): Promise<string> {
    try {
      const { stdout } = await execAsync(`git diff -- "${filePath}"`, {
        cwd: this.projectPath,
      });
      return stdout;
    } catch {
      return "";
    }
  }

  /**
   * Obtém o diff de um arquivo em relação ao HEAD (o que será commitado após git add -A)
   */
  async getFileDiffHead(filePath: string): Promise<string> {
    try {
      const { stdout } = await execAsync(`git diff HEAD -- "${filePath}"`, {
        cwd: this.projectPath,
      });
      return stdout;
    } catch {
      return "";
    }
  }

  /**
   * Lista arquivos do repositório (respeitando .gitignore)
   */
  async listTrackedFiles(maxFiles: number = 500): Promise<string[]> {
    try {
      const { stdout } = await execAsync(
        `git ls-files --cached --others --exclude-standard | head -${maxFiles}`,
        { cwd: this.projectPath },
      );
      return stdout.trim().split("\n").filter(Boolean);
    } catch {
      return [];
    }
  }

  /**
   * Lê o conteúdo de múltiplos arquivos
   */
  async readFiles(filePaths: string[]): Promise<Record<string, string>> {
    const contents: Record<string, string> = {};

    for (const filePath of filePaths) {
      try {
        const fullPath = path.join(this.projectPath, filePath);
        const content = await fs.promises.readFile(fullPath, "utf-8");
        contents[filePath] = content;
      } catch {
        // Ignora arquivos que não podem ser lidos
      }
    }

    return contents;
  }

  /**
   * Aplica um patch unificado via git apply.
   * Retorna { success: true } ou { success: false, error: string }.
   */
  private async applyPatch(unifiedDiff: string): Promise<{
    success: boolean;
    error?: string;
  }> {
    const tmpFile = path.join(
      os.tmpdir(),
      `dcc-patch-${Date.now()}-${Math.random().toString(36).slice(2)}.patch`,
    );
    try {
      await fs.promises.writeFile(tmpFile, unifiedDiff, "utf-8");
      const env = { ...process.env, GIT_TERMINAL_PROMPT: "0" };
      await execFileAsync("git", ["apply", "--check", tmpFile], {
        cwd: this.projectPath,
        env,
      });
      await execFileAsync("git", ["apply", tmpFile], {
        cwd: this.projectPath,
        env,
      });
      return { success: true };
    } catch (err: unknown) {
      const message =
        err instanceof Error ? err.message : String(err);
      const stderr =
        err && typeof err === "object" && "stderr" in err
          ? String((err as { stderr?: string }).stderr ?? "")
          : "";
      return {
        success: false,
        error: stderr.trim() || message,
      };
    } finally {
      try {
        await fs.promises.unlink(tmpFile);
      } catch {
        // ignore cleanup errors
      }
    }
  }

  /**
   * Aplica mudanças de código ao repositório.
   * Ordem preferida: git apply (quando diff válido) → escrita de arquivo (fallback).
   */
  async applyChanges(
    changes: CodeSuggestion[],
    options: { createBackup?: boolean; dryRun?: boolean } = {},
  ): Promise<ApplyChangesResult> {
    const appliedFiles: string[] = [];
    const appliedVia: Array<{ path: string; via: "git-apply" | "file-write" }> =
      [];
    const failedFiles: Array<{ path: string; error: string }> = [];
    let backupPath: string | undefined;

    // Criar backup se solicitado (antes de qualquer alteração)
    if (options.createBackup && !options.dryRun) {
      backupPath = await this.createBackup(changes.map((c) => c.path));
    }

    for (const change of changes) {
      try {
        const fullPath = path.join(this.projectPath, change.path);
        const dir = path.dirname(fullPath);

        if (options.dryRun) {
          console.log(`[DRY RUN] Would ${change.action}: ${change.path}`);
          appliedFiles.push(change.path);
          appliedVia.push({
            path: change.path,
            via: looksLikeUnifiedDiff(change.diff ?? "") ? "git-apply" : "file-write",
          });
          continue;
        }

        const hasDiff =
          (change.diff ?? "").trim().length > 0 &&
          looksLikeUnifiedDiff(change.diff ?? "");
        const hasContent =
          (change.suggestedContent ?? "").trim().length > 0;

        // 1. Tentar git apply quando há diff válido (create/modify/delete)
        if (hasDiff) {
          const result = await this.applyPatch(change.diff!);
          if (result.success) {
            appliedFiles.push(change.path);
            appliedVia.push({ path: change.path, via: "git-apply" });
            continue;
          }
          // git apply falhou — seguir para fallback
        }

        // 2. Fallback: escrita de arquivo com suggestedContent
        switch (change.action) {
          case "create":
            if (hasContent) {
              await fs.promises.mkdir(dir, { recursive: true });
              await fs.promises.writeFile(
                fullPath,
                change.suggestedContent!,
                "utf-8",
              );
              appliedFiles.push(change.path);
              appliedVia.push({ path: change.path, via: "file-write" });
            } else if (hasDiff) {
              failedFiles.push({
                path: change.path,
                error: "git apply failed and no suggestedContent to fallback",
              });
            }
            break;

          case "modify":
            if (hasContent) {
              await fs.promises.writeFile(
                fullPath,
                change.suggestedContent!,
                "utf-8",
              );
              appliedFiles.push(change.path);
              appliedVia.push({ path: change.path, via: "file-write" });
            } else if (hasDiff) {
              failedFiles.push({
                path: change.path,
                error: "git apply failed and no suggestedContent to fallback",
              });
            }
            break;

          case "delete":
            if (fs.existsSync(fullPath)) {
              await fs.promises.unlink(fullPath);
              appliedFiles.push(change.path);
              appliedVia.push({ path: change.path, via: "file-write" });
            }
            break;
        }
      } catch (error) {
        failedFiles.push({
          path: change.path,
          error: error instanceof Error ? error.message : "Unknown error",
        });
      }
    }

    return {
      success: failedFiles.length === 0,
      appliedFiles,
      failedFiles,
      backupPath,
      appliedVia,
    };
  }

  /**
   * Garante que o .gitignore do projeto ignore .dcc-backups (evita commit acidental).
   * Falha em silêncio para não impactar o backup.
   */
  private async ensureGitignoreIgnoresDccBackups(
    projectPath: string,
  ): Promise<void> {
    try {
      const gitignorePath = path.join(projectPath, ".gitignore");
      const dccBackupPatterns = [
        ".dcc-backups",
        "/.dcc-backups/",
        ".dcc-backups/",
      ];
      const hasIgnore = (content: string) =>
        content.split(/\r?\n/).some((line) => {
          const trimmed = line.replace(/#.*$/, "").trim();
          return trimmed && dccBackupPatterns.some((p) => trimmed === p);
        });

      if (fs.existsSync(gitignorePath)) {
        const content = await fs.promises.readFile(gitignorePath, "utf-8");
        if (hasIgnore(content)) return;
        const suffix = content.trimEnd().length ? "\n" : "";
        await fs.promises.appendFile(
          gitignorePath,
          `${suffix}# DevCommandCenter backups\n.dcc-backups\n`,
          "utf-8",
        );
      } else {
        await fs.promises.writeFile(
          gitignorePath,
          "# DevCommandCenter backups\n.dcc-backups\n",
          "utf-8",
        );
      }
    } catch {
      // Ignora erros; backup já foi criado
    }
  }

  /**
   * Cria backup dos arquivos antes de aplicar mudanças
   */
  private async createBackup(filePaths: string[]): Promise<string> {
    const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
    const backupDir = path.join(this.projectPath, ".dcc-backups", timestamp);
    await fs.promises.mkdir(backupDir, { recursive: true });

    await this.ensureGitignoreIgnoresDccBackups(this.projectPath);

    for (const filePath of filePaths) {
      try {
        const sourcePath = path.join(this.projectPath, filePath);
        if (fs.existsSync(sourcePath)) {
          const destPath = path.join(backupDir, filePath);
          await fs.promises.mkdir(path.dirname(destPath), { recursive: true });
          await fs.promises.copyFile(sourcePath, destPath);
        }
      } catch {
        // Ignora erros de backup
      }
    }

    return backupDir;
  }

  /**
   * Verifica se o projeto está em um worktree (vs. repositório principal)
   * e retorna o worktree root quando aplicável.
   */
  async getWorktreeInfo(): Promise<{
    isWorktree: boolean;
    worktreeRoot?: string;
  }> {
    try {
      const isRepo = await this.isGitRepo();
      if (!isRepo) return { isWorktree: false };

      const { stdout: toplevel } = await execAsync(
        "git rev-parse --show-toplevel",
        { cwd: this.projectPath },
      );
      const ourRoot = path.resolve(this.projectPath, toplevel.trim());

      const { stdout } = await execAsync("git worktree list --porcelain", {
        cwd: this.projectPath,
      });
      const blocks = stdout.split(/\n\n+/).filter(Boolean);
      const worktreePaths: string[] = [];
      for (const block of blocks) {
        const m = block.match(/^worktree\s+(.+)$/m);
        if (m) worktreePaths.push(path.resolve(m[1].trim()));
      }
      if (worktreePaths.length === 0) return { isWorktree: false };
      const mainRoot = worktreePaths[0];
      const inMain = ourRoot === mainRoot;
      const inOther = worktreePaths.slice(1).some((p) => p === ourRoot);
      const isWorktree = !inMain && inOther;
      return {
        isWorktree,
        worktreeRoot: isWorktree ? ourRoot : undefined,
      };
    } catch {
      return { isWorktree: false };
    }
  }

  /**
   * Descobre o branch padrão do repositório (main ou master).
   * Usa origin/HEAD se existir; senão tenta main e depois master.
   */
  async getDefaultBranch(): Promise<string> {
    try {
      const { stdout: originHead } = await execAsync(
        "git symbolic-ref refs/remotes/origin/HEAD",
        { cwd: this.projectPath, encoding: "utf8" },
      );
      const ref = originHead.trim();
      if (ref) {
        const match = ref.match(/^refs\/remotes\/origin\/(.+)$/);
        if (match?.[1]) return match[1];
      }
    } catch {
      // origin/HEAD não configurado, tenta main/master
    }
    try {
      await execAsync("git rev-parse --verify main", {
        cwd: this.projectPath,
      });
      return "main";
    } catch {
      try {
        await execAsync("git rev-parse --verify master", {
          cwd: this.projectPath,
        });
        return "master";
      } catch {
        throw new Error("Nenhum branch padrão (main/master) encontrado.");
      }
    }
  }

  /**
   * Cria um novo branch. Se fromBranch for informado, cria a partir dele (ex.: main/master).
   */
  async createBranch(branchName: string, fromBranch?: string): Promise<boolean> {
    try {
      if (fromBranch) {
        await execAsync(`git checkout "${fromBranch}"`, {
          cwd: this.projectPath,
        });
      }
      await execAsync(`git checkout -b "${branchName}"`, {
        cwd: this.projectPath,
      });
      return true;
    } catch {
      return false;
    }
  }

  /**
   * Faz commit das mudanças
   */
  async commit(message: string, files?: string[]): Promise<boolean> {
    try {
      if (files && files.length > 0) {
        const fileList = files.map((f) => `"${f}"`).join(" ");
        await execAsync(`git add ${fileList}`, { cwd: this.projectPath });
      } else {
        await execAsync("git add -A", { cwd: this.projectPath });
      }
      await execAsync(`git commit -m "${message}"`, { cwd: this.projectPath });
      return true;
    } catch {
      return false;
    }
  }

  /**
   * Executa git reset --hard para descartar alterações ou reverter último commit.
   * @param ref "HEAD" = descarta alterações não commitadas; "HEAD~1" = reverte último commit
   */
  async reset(ref: "HEAD" | "HEAD~1" = "HEAD"): Promise<{ success: boolean; error?: string }> {
    try {
      await execAsync(`git reset --hard ${ref}`, { cwd: this.projectPath });
      return { success: true };
    } catch (err: unknown) {
      let message = "Erro ao reverter alterações.";
      if (err instanceof Error) {
        message = err.message;
      } else if (err && typeof err === "object" && "stderr" in err) {
        message = String((err as { stderr?: string }).stderr ?? "").trim();
      }
      return {
        success: false,
        error: message || "Erro ao reverter alterações.",
      };
    }
  }

  /**
   * Envia commits do branch atual para o remoto (origin)
   */
  async push(): Promise<{ success: boolean; error?: string }> {
    try {
      const branch = await this.getCurrentBranch();
      if (!branch || branch === "unknown" || branch === "HEAD") {
        return { success: false, error: "Branch atual não identificado." };
      }
      await execAsync(`git push origin ${branch}`, {
        cwd: this.projectPath,
      });
      return { success: true };
    } catch (err: unknown) {
      let message = "Erro ao fazer push.";
      if (err instanceof Error) {
        message = err.message;
      } else if (err && typeof err === "object" && "stderr" in err) {
        message = String((err as { stderr?: string }).stderr ?? "").trim();
      }
      return {
        success: false,
        error: message || "Erro ao fazer push.",
      };
    }
  }
}

// Factory function
export function createGitService(projectPath: string): GitService {
  return new GitService(projectPath);
}
