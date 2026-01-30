/**
 * Git Service - Serviço para operações Git
 *
 * Fornece contexto do repositório para a IA e aplica mudanças
 */

import { exec, execSync } from "node:child_process";
import { promisify } from "node:util";
import * as fs from "node:fs";
import * as path from "node:path";
import type {
  GitInfo,
  GitStatus,
  GitCommit,
  CodeSuggestion,
  ApplyChangesResult,
} from "./types";

const execAsync = promisify(exec);

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
   * Aplica mudanças de código ao repositório
   */
  async applyChanges(
    changes: CodeSuggestion[],
    options: { createBackup?: boolean; dryRun?: boolean } = {},
  ): Promise<ApplyChangesResult> {
    const appliedFiles: string[] = [];
    const failedFiles: Array<{ path: string; error: string }> = [];
    let backupPath: string | undefined;

    // Criar backup se solicitado
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
          continue;
        }

        switch (change.action) {
          case "create":
            await fs.promises.mkdir(dir, { recursive: true });
            await fs.promises.writeFile(
              fullPath,
              change.suggestedContent || "",
              "utf-8",
            );
            appliedFiles.push(change.path);
            break;

          case "modify":
            if (change.suggestedContent) {
              await fs.promises.writeFile(
                fullPath,
                change.suggestedContent,
                "utf-8",
              );
              appliedFiles.push(change.path);
            }
            break;

          case "delete":
            if (fs.existsSync(fullPath)) {
              await fs.promises.unlink(fullPath);
              appliedFiles.push(change.path);
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
   * Cria um novo branch
   */
  async createBranch(branchName: string): Promise<boolean> {
    try {
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
