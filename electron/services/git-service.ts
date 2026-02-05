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
import * as crypto from "node:crypto";
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
      line.startsWith("+++ ")
  );
}

/**
 * Validates a unified diff structure before attempting git apply.
 * This saves I/O by avoiding temp file creation for malformed diffs.
 *
 * Returns { valid: true } if the diff has valid structure, or
 * { valid: false, reason: string } if invalid.
 */
function validateDiff(diff: string): { valid: boolean; reason?: string } {
  const trimmed = diff.trim();
  if (!trimmed) {
    return { valid: false, reason: "Empty diff" };
  }

  const lines = trimmed.split(/\r?\n/);

  // Check for basic unified diff structure
  let hasOldHeader = false;
  let hasNewHeader = false;
  let hasHunk = false;
  let hasChanges = false;

  for (const line of lines) {
    // Check for file headers
    if (line.startsWith("--- ")) {
      hasOldHeader = true;
    } else if (line.startsWith("+++ ")) {
      hasNewHeader = true;
    }
    // Check for hunk header (@@ -x,y +x,y @@)
    else if (line.match(/^@@\s+-\d+(?:,\d+)?\s+\+\d+(?:,\d+)?\s+@@/)) {
      hasHunk = true;
    }
    // Check for actual changes (additions or deletions)
    else if (line.startsWith("+") && !line.startsWith("+++")) {
      hasChanges = true;
    } else if (line.startsWith("-") && !line.startsWith("---")) {
      hasChanges = true;
    }
  }

  // Validate minimum requirements
  if (!hasOldHeader) {
    return { valid: false, reason: "Missing old file header (--- a/...)" };
  }
  if (!hasNewHeader) {
    return { valid: false, reason: "Missing new file header (+++ b/...)" };
  }
  if (!hasHunk) {
    return { valid: false, reason: "Missing hunk header (@@ -x,y +x,y @@)" };
  }
  if (!hasChanges) {
    return { valid: false, reason: "No changes in diff (no + or - lines)" };
  }

  // Optional: Validate hunk line counts match actual content
  // This is a more thorough validation but may be too strict for some LLM outputs
  const hunkValidation = validateHunkLineCounts(lines);
  if (!hunkValidation.valid) {
    return hunkValidation;
  }

  return { valid: true };
}

/**
 * Validates that hunk line counts roughly match actual content.
 * Allows some tolerance for LLM-generated diffs that may have minor inconsistencies.
 */
function validateHunkLineCounts(lines: string[]): {
  valid: boolean;
  reason?: string;
} {
  let inHunk = false;
  let expectedOld = 0;
  let expectedNew = 0;
  let actualOld = 0;
  let actualNew = 0;

  for (const line of lines) {
    const hunkMatch = line.match(
      /^@@\s+-(\d+)(?:,(\d+))?\s+\+(\d+)(?:,(\d+))?\s+@@/
    );

    if (hunkMatch) {
      // If we were in a previous hunk, check its counts
      if (inHunk) {
        // Allow some tolerance (within 20% or 5 lines, whichever is greater)
        const tolerance = Math.max(5, Math.floor(expectedOld * 0.2));
        if (
          Math.abs(actualOld - expectedOld) > tolerance ||
          Math.abs(actualNew - expectedNew) > tolerance
        ) {
          return {
            valid: false,
            reason: `Hunk line count mismatch: expected ${expectedOld}/${expectedNew}, got ${actualOld}/${actualNew}`,
          };
        }
      }

      // Start new hunk
      inHunk = true;
      expectedOld = parseInt(hunkMatch[2] ?? "1", 10);
      expectedNew = parseInt(hunkMatch[4] ?? "1", 10);
      actualOld = 0;
      actualNew = 0;
    } else if (inHunk) {
      if (line.startsWith("+") && !line.startsWith("+++")) {
        actualNew++;
      } else if (line.startsWith("-") && !line.startsWith("---")) {
        actualOld++;
      } else if (line.startsWith(" ") || line === "") {
        // Context line counts for both
        actualOld++;
        actualNew++;
      }
    }
  }

  // Check final hunk
  if (inHunk) {
    const tolerance = Math.max(5, Math.floor(expectedOld * 0.2));
    if (
      Math.abs(actualOld - expectedOld) > tolerance ||
      Math.abs(actualNew - expectedNew) > tolerance
    ) {
      return {
        valid: false,
        reason: `Final hunk line count mismatch: expected ${expectedOld}/${expectedNew}, got ${actualOld}/${actualNew}`,
      };
    }
  }

  return { valid: true };
}

/**
 * Extrai as linhas adicionadas (+) de um diff unificado.
 * Ignora headers (+++) e retorna apenas o conteúdo das linhas.
 */
function extractAddedLinesFromDiff(diff: string): string[] {
  const lines = diff.split(/\r?\n/);
  const addedLines: string[] = [];
  for (const line of lines) {
    // Ignora headers (+++ b/path)
    if (line.startsWith("+++")) continue;
    // Linha adicionada: começa com + (não é header)
    if (line.startsWith("+")) {
      addedLines.push(line.slice(1)); // Remove o + do início
    }
  }
  return addedLines;
}

/**
 * Verifica se as linhas adicionadas de um diff já estão presentes no conteúdo do arquivo.
 * Retorna true se TODAS as linhas adicionadas estão presentes (diff já aplicado).
 * Retorna false se alguma linha está faltando.
 */
function checkDiffAlreadyApplied(
  currentContent: string,
  diff: string
): boolean {
  const addedLines = extractAddedLinesFromDiff(diff);
  if (addedLines.length === 0) return false; // Sem linhas adicionadas, não pode verificar

  // Normaliza para comparação (trim de cada linha)
  const contentLines = currentContent.split(/\r?\n/).map((l) => l.trimEnd());

  // Verifica se TODAS as linhas adicionadas estão presentes no arquivo
  for (const addedLine of addedLines) {
    const normalizedAdded = addedLine.trimEnd();
    // Linha vazia sempre "existe"
    if (normalizedAdded === "") continue;
    // Verifica se a linha existe no arquivo
    if (!contentLines.some((cl) => cl === normalizedAdded)) {
      return false; // Linha não encontrada, diff não foi aplicado
    }
  }
  return true; // Todas as linhas encontradas
}

/**
 * Normaliza os paths do header do diff para corresponder ao targetPath.
 * LLMs frequentemente geram paths incorretos no header (ex.: a/src/file.ts vs a/file.ts).
 */
function normalizeDiffPaths(diff: string, targetPath: string): string {
  return diff
    .split("\n")
    .map((line) => {
      if (line.startsWith("--- ")) return `--- a/${targetPath}`;
      if (line.startsWith("+++ ")) return `+++ b/${targetPath}`;
      return line;
    })
    .join("\n");
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
        { cwd: this.projectPath }
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
        { cwd: this.projectPath }
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
      `dcc-patch-${Date.now()}-${Math.random().toString(36).slice(2)}.patch`
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
      const message = err instanceof Error ? err.message : String(err);
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
   * Ordem preferida:
   * 1. Verificar se diff já foi aplicado (CLIs como Cursor/Claude aplicam automaticamente)
   * 2. Tentar git apply (quando diff válido)
   * 3. Fallback: escrita de arquivo com suggestedContent
   */
  async applyChanges(
    changes: CodeSuggestion[],
    options: { createBackup?: boolean; dryRun?: boolean } = {}
  ): Promise<ApplyChangesResult> {
    const appliedFiles: string[] = [];
    const appliedVia: Array<{
      path: string;
      via: "git-apply" | "file-write" | "already-applied";
    }> = [];
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
            via: looksLikeUnifiedDiff(change.diff ?? "")
              ? "git-apply"
              : "file-write",
          });
          continue;
        }

        const hasDiff =
          (change.diff ?? "").trim().length > 0 &&
          looksLikeUnifiedDiff(change.diff ?? "");
        const hasContent = (change.suggestedContent ?? "").trim().length > 0;

        // 1. Para modify/create com diff, verificar se já foi aplicado
        // (CLIs como Cursor Agent e Claude Code aplicam automaticamente)
        if (
          hasDiff &&
          (change.action === "modify" || change.action === "create")
        ) {
          // Verificar se o arquivo existe e se as mudanças já estão lá
          if (fs.existsSync(fullPath)) {
            try {
              const currentContent = await fs.promises.readFile(
                fullPath,
                "utf-8"
              );
              if (checkDiffAlreadyApplied(currentContent, change.diff!)) {
                // Diff já aplicado (provavelmente pelo CLI)
                appliedFiles.push(change.path);
                appliedVia.push({ path: change.path, via: "already-applied" });
                continue;
              }
            } catch {
              // Erro ao ler arquivo, seguir para git apply
            }
          }
        }

        // 2. Tentar git apply quando há diff válido
        if (hasDiff) {
          // Pre-validate diff structure before creating temp file (performance optimization)
          const validation = validateDiff(change.diff!);
          if (!validation.valid) {
            // Skip git apply and go directly to fallback (saves I/O)
            console.log(
              `[GitService] Skipping git apply for ${change.path}: ${validation.reason}`
            );
          } else {
            const normalizedDiff = normalizeDiffPaths(
              change.diff!,
              change.path
            );
            const result = await this.applyPatch(normalizedDiff);
            if (result.success) {
              appliedFiles.push(change.path);
              appliedVia.push({ path: change.path, via: "git-apply" });
              continue;
            }
            // git apply falhou — verificar se foi aplicado de outra forma
            // (pode ter sido aplicado pelo CLI entre a verificação e o apply)
            if (
              fs.existsSync(fullPath) &&
              (change.action === "modify" || change.action === "create")
            ) {
              try {
                const currentContent = await fs.promises.readFile(
                  fullPath,
                  "utf-8"
                );
                if (checkDiffAlreadyApplied(currentContent, change.diff!)) {
                  appliedFiles.push(change.path);
                  appliedVia.push({
                    path: change.path,
                    via: "already-applied",
                  });
                  continue;
                }
              } catch {
                // Erro ao ler, seguir para fallback
              }
            }
          }
          // git apply falhou/skipped e não foi aplicado — seguir para fallback
        }

        // 3. Fallback: escrita de arquivo com suggestedContent (using atomic writes)
        switch (change.action) {
          case "create":
            if (hasContent) {
              // Use atomic write for safety
              await this.atomicWriteFile(fullPath, change.suggestedContent!);
              appliedFiles.push(change.path);
              appliedVia.push({ path: change.path, via: "file-write" });
            } else if (hasDiff) {
              // Final check: CLI may have already applied the file (e.g. Claude/Cursor/Codex CLI)
              if (fs.existsSync(fullPath)) {
                try {
                  const currentContent = await fs.promises.readFile(
                    fullPath,
                    "utf-8"
                  );
                  if (checkDiffAlreadyApplied(currentContent, change.diff!)) {
                    appliedFiles.push(change.path);
                    appliedVia.push({
                      path: change.path,
                      via: "already-applied",
                    });
                    break;
                  }
                } catch {
                  // Fall through to failedFiles
                }
              }
              failedFiles.push({
                path: change.path,
                error: "git apply failed and no suggestedContent to fallback",
              });
            }
            break;

          case "modify":
            if (hasContent) {
              // Use atomic write for safety
              await this.atomicWriteFile(fullPath, change.suggestedContent!);
              appliedFiles.push(change.path);
              appliedVia.push({ path: change.path, via: "file-write" });
            } else if (hasDiff) {
              // Final check: CLI may have already applied the file (e.g. Claude/Cursor/Codex CLI)
              if (fs.existsSync(fullPath)) {
                try {
                  const currentContent = await fs.promises.readFile(
                    fullPath,
                    "utf-8"
                  );
                  if (checkDiffAlreadyApplied(currentContent, change.diff!)) {
                    appliedFiles.push(change.path);
                    appliedVia.push({
                      path: change.path,
                      via: "already-applied",
                    });
                    break;
                  }
                } catch {
                  // Fall through to failedFiles
                }
              }
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
    projectPath: string
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
          "utf-8"
        );
      } else {
        await fs.promises.writeFile(
          gitignorePath,
          "# DevCommandCenter backups\n.dcc-backups\n",
          "utf-8"
        );
      }
    } catch {
      // Ignora erros; backup já foi criado
    }
  }

  /**
   * Computes MD5 hash of file content for comparison
   */
  private async computeFileHash(filePath: string): Promise<string | null> {
    try {
      const content = await fs.promises.readFile(filePath);
      return crypto.createHash("md5").update(content).digest("hex");
    } catch {
      return null;
    }
  }

  /**
   * Writes content atomically using temp file + rename pattern.
   * This prevents partial writes if the process is interrupted.
   */
  private async atomicWriteFile(
    filePath: string,
    content: string
  ): Promise<void> {
    const dir = path.dirname(filePath);
    const tempFile = path.join(
      dir,
      `.${path.basename(filePath)}.${Date.now()}.tmp`
    );

    try {
      // Ensure directory exists
      await fs.promises.mkdir(dir, { recursive: true });
      // Write to temp file
      await fs.promises.writeFile(tempFile, content, "utf-8");
      // Atomic rename
      await fs.promises.rename(tempFile, filePath);
    } catch (error) {
      // Clean up temp file if it exists
      try {
        await fs.promises.unlink(tempFile);
      } catch {
        // Ignore cleanup errors
      }
      throw error;
    }
  }

  /**
   * Cria backup dos arquivos antes de aplicar mudanças.
   * Optimized: Uses incremental backup - only copies files that have changed.
   */
  private async createBackup(filePaths: string[]): Promise<string> {
    const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
    const backupDir = path.join(this.projectPath, ".dcc-backups", timestamp);
    await fs.promises.mkdir(backupDir, { recursive: true });

    await this.ensureGitignoreIgnoresDccBackups(this.projectPath);

    // Track which files were actually backed up (for potential cleanup of empty backups)
    let filesBackedUp = 0;

    // Get previous backup directory for hash comparison (incremental backup)
    const backupsRoot = path.join(this.projectPath, ".dcc-backups");
    let previousBackupDir: string | null = null;
    try {
      const existingBackups = await fs.promises.readdir(backupsRoot);
      const sortedBackups = existingBackups
        .filter((name) => name !== timestamp)
        .sort()
        .reverse();
      if (sortedBackups.length > 0) {
        previousBackupDir = path.join(backupsRoot, sortedBackups[0]);
      }
    } catch {
      // No previous backups exist
    }

    for (const filePath of filePaths) {
      try {
        const sourcePath = path.join(this.projectPath, filePath);
        if (!fs.existsSync(sourcePath)) continue;

        const destPath = path.join(backupDir, filePath);

        // Incremental backup: check if file changed since last backup
        if (previousBackupDir) {
          const previousBackupFile = path.join(previousBackupDir, filePath);
          if (fs.existsSync(previousBackupFile)) {
            const [currentHash, previousHash] = await Promise.all([
              this.computeFileHash(sourcePath),
              this.computeFileHash(previousBackupFile),
            ]);

            // Skip if file hasn't changed (same hash)
            if (currentHash && previousHash && currentHash === previousHash) {
              // Create hardlink to previous backup instead of copying
              // This saves disk space and I/O
              try {
                await fs.promises.mkdir(path.dirname(destPath), {
                  recursive: true,
                });
                await fs.promises.link(previousBackupFile, destPath);
                filesBackedUp++;
                continue;
              } catch {
                // Fall through to normal copy if hardlink fails
              }
            }
          }
        }

        // Normal copy for changed or new files
        await fs.promises.mkdir(path.dirname(destPath), { recursive: true });
        await fs.promises.copyFile(sourcePath, destPath);
        filesBackedUp++;
      } catch {
        // Ignora erros de backup individual
      }
    }

    // Clean up empty backup directory if no files were backed up
    if (filesBackedUp === 0) {
      try {
        await fs.promises.rmdir(backupDir);
      } catch {
        // Ignore cleanup errors
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
        { cwd: this.projectPath }
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
        { cwd: this.projectPath, encoding: "utf8" }
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
  async createBranch(
    branchName: string,
    fromBranch?: string
  ): Promise<boolean> {
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
  async reset(
    ref: "HEAD" | "HEAD~1" = "HEAD"
  ): Promise<{ success: boolean; error?: string }> {
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
