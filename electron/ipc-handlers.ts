import { app, dialog, shell, BrowserWindow, Notification } from "electron";
import type { IpcMain } from "electron";
import { autoUpdater } from "electron-updater";
import fs from "node:fs";
import { execSync, execFileSync, spawn } from "node:child_process";
import { platform } from "node:os";
import db from "../lib/database";
import { getActivation, setActivation } from "../lib/database/activation";
import { aiOrchestrator, GitService } from "./services";
import {
  createWorktreeForMission,
  mergeWorktreeIntoMain,
  discardWorktree,
} from "./services/worktree-service";
import { getMachineId } from "./services/machine-id";
import {
  providerService,
  sanitizeForRenderer,
} from "./services/provider-service";
import { detectCliPath, validateCliPath } from "./services/cli-detection";
import {
  spawnPty,
  getOrCreatePty,
  getMissionSession,
  onTerminalSessionEvent,
  writeToPty,
  resizePty,
  killPty,
  killPtyByMissionId,
} from "./services/terminal-pty-service";

const BETA_ACTIVATE_URL = "https://www.devcommandcenter.com/api/beta-activate";
const isDev = process.env.NODE_ENV === "development" || !app.isPackaged;

export function registerIpcHandlers(ipcMain: IpcMain) {
  const terminalSessionFlushTimers = new Map<string, NodeJS.Timeout>();

  const snapshotGitForMission = async (missionId: string) => {
    const mission = db.missions.findById(missionId);
    if (!mission) return null;
    const project = db.projects.findById(mission.projectId);
    if (!project) return null;
    const targetPath = mission.worktreePath ?? project.path;
    try {
      const branchState = await new GitService(targetPath).getBranchState();
      return {
        branch: branchState.branch,
        upstreamBranch: branchState.upstreamBranch ?? null,
        defaultBranch: branchState.defaultBranch ?? null,
        isRepo: branchState.isRepo,
        isDirty: branchState.isDirty,
        changedFiles: branchState.changedFiles,
        stagedCount: branchState.staged.length,
        unstagedCount: branchState.unstaged.length,
        untrackedCount: branchState.untracked.length,
        aheadCount: branchState.aheadCount,
        behindCount: branchState.behindCount,
        aheadOfDefaultCount: branchState.aheadOfDefaultCount,
        behindOfDefaultCount: branchState.behindOfDefaultCount,
        hasUpstream: branchState.hasUpstream,
        mergeReadiness: branchState.mergeReadiness,
      };
    } catch {
      return null;
    }
  };

  const persistMissionSession = async (missionId: string) => {
    const mission = db.missions.findById(missionId);
    if (!mission) return;
    const session = getMissionSession(missionId);
    const gitSnapshot = await snapshotGitForMission(missionId);
    db.missions.update(missionId, {
      context: {
        ...(mission.context ?? { files: [] }),
        files: mission.context?.files ?? [],
        agentSession: session,
        gitSnapshot,
      },
    });
  };

  const schedulePersistMissionSession = (missionId: string) => {
    const existingTimer = terminalSessionFlushTimers.get(missionId);
    if (existingTimer) clearTimeout(existingTimer);
    const nextTimer = setTimeout(() => {
      terminalSessionFlushTimers.delete(missionId);
      void persistMissionSession(missionId);
    }, 400);
    terminalSessionFlushTimers.set(missionId, nextTimer);
  };

  onTerminalSessionEvent((event) => {
    if (!event.missionId) return;
    if (event.type === "data") {
      schedulePersistMissionSession(event.missionId);
      return;
    }

    void persistMissionSession(event.missionId);

    if (event.type === "started") {
      const mission = db.missions.findById(event.missionId);
      if (
        mission &&
        !["planning", "generating_code", "applying"].includes(mission.status) &&
        !["completed", "failed", "cancelled"].includes(mission.status)
      ) {
        db.missions.start(event.missionId);
      } else if (
        mission &&
        ["completed", "failed", "cancelled"].includes(mission.status)
      ) {
        db.missions.update(event.missionId, {
          status: "planning",
          startedAt: new Date(),
          completedAt: null,
          errorMessage: null,
        });
      }
      return;
    }

    if (event.type === "exit") {
      const mission = db.missions.findById(event.missionId);
      if (!mission || ["completed", "failed", "cancelled"].includes(mission.status)) return;
      if ((event.exitCode ?? 0) === 0) {
        db.missions.complete(
          event.missionId,
          `Agent finalizado com sucesso (exit code ${event.exitCode ?? 0})`,
        );
      } else {
        db.missions.fail(
          event.missionId,
          `Agent finalizado com falha (exit code ${event.exitCode ?? -1})`,
        );
      }
    }
  });

  // ==========================================
  // App update (only when packaged)
  // ==========================================
  ipcMain.handle("app:getVersion", () => {
    return app.getVersion();
  });

  ipcMain.handle("app:checkForUpdates", async () => {
    if (isDev || !app.isPackaged) return;
    await autoUpdater.checkForUpdates();
  });

  ipcMain.handle("app:quitAndInstall", () => {
    if (isDev || !app.isPackaged) return;
    autoUpdater.quitAndInstall(false, true);
  });

  ipcMain.handle(
    "app:showNotification",
    (_event, payload: { title: string; body?: string }) => {
      if (!Notification.isSupported()) return;
      const n = new Notification({
        title: payload.title ?? "Dev Command Center",
        body: payload.body,
      });
      n.show();
    },
  );

  // ==========================================
  // Dialog handlers
  // ==========================================
  ipcMain.handle("dialog:selectDirectory", async () => {
    const result = await dialog.showOpenDialog({
      properties: ["openDirectory", "createDirectory"],
      title: "Select Project Directory",
    });
    return result.canceled ? null : result.filePaths[0];
  });

  ipcMain.handle("dialog:showMessage", async (_event, options) => {
    const result = await dialog.showMessageBox(options);
    return result.response;
  });

  ipcMain.handle("dialog:confirm", async (_event, message: string) => {
    const result = await dialog.showMessageBox({
      type: "question",
      buttons: ["Cancel", "Confirm"],
      defaultId: 1,
      cancelId: 0,
      message,
    });
    return result.response === 1;
  });

  // ==========================================
  // Shell handlers
  // ==========================================
  ipcMain.handle("shell:openExternal", async (_event, url: string) => {
    await shell.openExternal(url);
  });

  ipcMain.handle("shell:openPath", async (_event, path: string) => {
    await shell.openPath(path);
  });

  ipcMain.handle("shell:showItemInFolder", (_event, path: string) => {
    shell.showItemInFolder(path);
  });

  ipcMain.handle(
    "shell:resolveCliPath",
    async (_event, command: string): Promise<{ path: string | null }> => {
      const safe = /^[a-zA-Z0-9-]+$/.test(command) ? command : "";
      if (!safe) return { path: null };

      const isWin = platform() === "win32";

      if (isWin) {
        try {
          const out = execSync(`where ${safe}`, { encoding: "utf8" });
          const firstLine = out.split(/\r?\n/)[0]?.trim();
          return { path: firstLine || null };
        } catch {
          return { path: null };
        }
      }

      // macOS/Linux: use login shell so we get the user's real PATH
      // (Electron often has a minimal PATH without Homebrew, ~/.local/bin, etc.)
      const userShell = process.env.SHELL || "zsh";
      try {
        const out = execSync(`${userShell} -l -c 'which ${safe}'`, {
          encoding: "utf8",
          timeout: 5000,
        });
        const firstLine = out.split(/\r?\n/)[0]?.trim();
        if (firstLine) return { path: firstLine };
      } catch {
        // which via login shell failed, try fallback paths
      }

      // Fallback: common install locations (Homebrew, /usr/local, ~/.local/bin)
      const home = process.env.HOME || process.env.USERPROFILE || "";
      const candidates = [
        "/opt/homebrew/bin/" + safe,
        "/usr/local/bin/" + safe,
        ...(home ? [home + "/.local/bin/" + safe] : []),
      ];
      for (const p of candidates) {
        try {
          if (fs.existsSync(p)) return { path: p };
        } catch {
          // continue
        }
      }
      return { path: null };
    },
  );

  ipcMain.handle(
    "shell:detectCliForProvider",
    async (_event, providerType: string): Promise<{ path: string | null }> => {
      const path = detectCliPath(providerType);
      return { path };
    },
  );

  ipcMain.handle(
    "shell:validateCliPath",
    async (
      _event,
      providerType: string,
      cliPath: string,
    ): Promise<{ valid: boolean; message?: string }> => {
      return validateCliPath(providerType, cliPath);
    },
  );

  // ==========================================
  // Window handlers
  // ==========================================
  ipcMain.handle("window:minimize", (event) => {
    BrowserWindow.fromWebContents(event.sender)?.minimize();
  });

  ipcMain.handle("window:maximize", (event) => {
    const win = BrowserWindow.fromWebContents(event.sender);
    if (win?.isMaximized()) {
      win.unmaximize();
    } else {
      win?.maximize();
    }
  });

  ipcMain.handle("window:close", (event) => {
    BrowserWindow.fromWebContents(event.sender)?.close();
  });

  ipcMain.handle("window:isMaximized", (event) => {
    return BrowserWindow.fromWebContents(event.sender)?.isMaximized() ?? false;
  });

  // ==========================================
  // License / activation (beta)
  // ==========================================
  ipcMain.handle("license:getStatus", () => {
    if (isDev) {
      const status = getActivation();
      // Em dev: se já tiver ativado, retorna; senão pode retornar activated true para bypass
      if (status?.activated) return status;
      return { activated: false, email: undefined };
    }
    return getActivation() ?? { activated: false, email: undefined };
  });

  ipcMain.handle("license:getMachineId", () => {
    const userDataPath = app.getPath("userData");
    return getMachineId(userDataPath);
  });

  ipcMain.handle(
    "license:activate",
    async (
      _event,
      email: string,
    ): Promise<{ success: boolean; message?: string }> => {
      const trimmed = String(email ?? "")
        .trim()
        .toLowerCase();
      if (!trimmed) {
        return { success: false, message: "Informe um e-mail válido." };
      }
      const machineId = getMachineId(app.getPath("userData"));

      try {
        const res = await fetch(BETA_ACTIVATE_URL, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ email: trimmed, machineId }),
        });

        const data = await res.json().catch(() => ({}));
        if (!res.ok) {
          const msg =
            (data && typeof data.message === "string" && data.message) ||
            `Falha na ativação (${res.status}). Tente novamente.`;
          return { success: false, message: msg };
        }

        if (data && data.ok === true) {
          setActivation({
            email: trimmed,
            machineId,
            token: data.token ?? null,
          });
          return { success: true };
        }

        return {
          success: false,
          message: (data && data.message) || "Resposta inválida do servidor.",
        };
      } catch (err) {
        const message =
          err instanceof Error
            ? err.message
            : "Erro de conexão. Verifique sua internet.";
        return { success: false, message };
      }
    },
  );

  ipcMain.handle("license:skipActivation", () => {
    if (!isDev) return { success: false };
    const machineId = getMachineId(app.getPath("userData"));
    setActivation({
      email: "dev@local",
      machineId,
      token: null,
    });
    return { success: true };
  });

  // ==========================================
  // Provider handlers (usam providerService para encrypt/decrypt)
  // ==========================================
  ipcMain.handle("db:providers:findAll", () => {
    return providerService.findAll().map(sanitizeForRenderer);
  });

  ipcMain.handle("db:providers:findById", (_event, id: string) => {
    const p = providerService.findById(id);
    return p ? sanitizeForRenderer(p) : null;
  });

  ipcMain.handle("db:providers:findByType", (_event, type: string) => {
    return providerService.findByType(type as any).map(sanitizeForRenderer);
  });

  ipcMain.handle("db:providers:findActive", () => {
    return providerService.findActive().map(sanitizeForRenderer);
  });

  ipcMain.handle("db:providers:create", (_event, data: any) => {
    const p = providerService.create(data);
    return sanitizeForRenderer(p);
  });

  ipcMain.handle("db:providers:update", (_event, id: string, data: any) => {
    const p = providerService.update(id, data);
    return p ? sanitizeForRenderer(p) : null;
  });

  ipcMain.handle("db:providers:delete", (_event, id: string) => {
    return providerService.delete(id);
  });

  ipcMain.handle(
    "db:providers:setActive",
    (_event, id: string, isActive: boolean) => {
      const p = providerService.setActive(id, isActive);
      return p ? sanitizeForRenderer(p) : null;
    },
  );

  ipcMain.handle("db:providers:testConnection", (_event, id: string) => {
    return providerService.testConnection(id);
  });

  ipcMain.handle("db:providers:isEncryptionAvailable", () => {
    return providerService.isEncryptionAvailable();
  });

  // ==========================================
  // Project handlers
  // ==========================================
  ipcMain.handle("db:projects:findAll", () => {
    return db.projects.findAll();
  });

  ipcMain.handle("db:projects:findById", (_event, id: string) => {
    return db.projects.findById(id);
  });

  ipcMain.handle("db:projects:findByPath", (_event, path: string) => {
    return db.projects.findByPath(path);
  });

  ipcMain.handle("db:projects:search", (_event, query: string) => {
    return db.projects.search(query);
  });

  ipcMain.handle("db:projects:create", (_event, data: any) => {
    return db.projects.create(data);
  });

  ipcMain.handle("db:projects:update", (_event, id: string, data: any) => {
    return db.projects.update(id, data);
  });

  ipcMain.handle("db:projects:delete", (_event, id: string) => {
    return db.projects.delete(id);
  });

  ipcMain.handle("db:projects:getStats", (_event, id: string) => {
    return db.projects.getStats(id);
  });

  ipcMain.handle("db:projects:updateLastOpened", (_event, id: string) => {
    return db.projects.updateLastOpened(id);
  });

  // ==========================================
  // Mission handlers
  // ==========================================
  ipcMain.handle("db:missions:findAll", () => {
    return db.missions.findAll();
  });

  ipcMain.handle("db:missions:findById", (_event, id: string) => {
    return db.missions.findById(id);
  });

  ipcMain.handle("db:missions:findByProject", (_event, projectId: string) => {
    return db.missions.findByProject(projectId);
  });

  ipcMain.handle("db:missions:findByStatus", (_event, status: string) => {
    return db.missions.findByStatus(status as any);
  });

  ipcMain.handle("db:missions:findActive", () => {
    return db.missions.findActive();
  });

  ipcMain.handle(
    "db:missions:search",
    (_event, query: string, projectId?: string) => {
      return db.missions.search(query, projectId);
    },
  );

  ipcMain.handle("db:missions:create", (_event, data: any) => {
    return db.missions.create(data);
  });

  ipcMain.handle("db:missions:update", (_event, id: string, data: any) => {
    return db.missions.update(id, data);
  });

  ipcMain.handle("db:missions:delete", (_event, id: string) => {
    return db.missions.delete(id);
  });

  ipcMain.handle(
    "db:missions:updateStatus",
    (_event, id: string, status: string) => {
      return db.missions.updateStatus(id, status as any);
    },
  );

  ipcMain.handle(
    "db:missions:updatePlan",
    (_event, id: string, plan: string | object) => {
      const planObj = typeof plan === "string" ? JSON.parse(plan) : plan;
      return db.missions.updatePlan(id, planObj);
    },
  );

  ipcMain.handle(
    "db:missions:updateGeneratedCode",
    (_event, id: string, code: string | object) => {
      const codeObj = typeof code === "string" ? JSON.parse(code) : code;
      return db.missions.updateGeneratedCode(id, codeObj);
    },
  );

  ipcMain.handle("db:missions:start", (_event, id: string) => {
    return db.missions.start(id);
  });

  ipcMain.handle(
    "db:missions:complete",
    (_event, id: string, summary?: string) => {
      return db.missions.complete(id, summary);
    },
  );

  ipcMain.handle("db:missions:fail", (_event, id: string, error: string) => {
    return db.missions.fail(id, error);
  });

  ipcMain.handle("db:missions:cancel", (_event, id: string) => {
    return db.missions.cancel(id);
  });

  ipcMain.handle("db:missions:getFullMission", (_event, id: string) => {
    return db.missions.getFullMission(id);
  });

  // ==========================================
  // Mission Log handlers
  // ==========================================
  ipcMain.handle(
    "db:missionLogs:findAll",
    (
      _event,
      options?: { missionId?: string; limit?: number; offset?: number },
    ) => {
      return db.missionLogs.findAll(options || { limit: 100, offset: 0 });
    },
  );

  ipcMain.handle("db:missionLogs:findById", (_event, id: string) => {
    return db.missionLogs.findById(id);
  });

  ipcMain.handle(
    "db:missionLogs:findByMission",
    (_event, missionId: string, limit?: number, offset?: number) => {
      return db.missionLogs.findByMission(missionId, limit, offset);
    },
  );

  ipcMain.handle(
    "db:missionLogs:findByLevel",
    (_event, level: string, missionId?: string) => {
      return db.missionLogs.findByLevel(level as any, missionId);
    },
  );

  ipcMain.handle(
    "db:missionLogs:search",
    (_event, query: string, missionId?: string) => {
      return db.missionLogs.search(query, missionId);
    },
  );

  ipcMain.handle("db:missionLogs:create", (_event, data: any) => {
    return db.missionLogs.create(data);
  });

  ipcMain.handle("db:missionLogs:delete", (_event, id: string) => {
    return db.missionLogs.delete(id);
  });

  ipcMain.handle(
    "db:missionLogs:deleteByMission",
    (_event, missionId: string) => {
      return db.missionLogs.deleteByMission(missionId);
    },
  );

  ipcMain.handle(
    "db:missionLogs:logInfo",
    (_event, missionId: string, message: string, metadata?: any) => {
      return db.missionLogs.logInfo(missionId, message, metadata);
    },
  );

  ipcMain.handle(
    "db:missionLogs:logWarning",
    (_event, missionId: string, message: string, metadata?: any) => {
      return db.missionLogs.logWarning(missionId, message, metadata);
    },
  );

  ipcMain.handle(
    "db:missionLogs:logError",
    (_event, missionId: string, message: string, metadata?: any) => {
      return db.missionLogs.logError(missionId, message, metadata);
    },
  );

  ipcMain.handle(
    "db:missionLogs:logDebug",
    (_event, missionId: string, message: string, metadata?: any) => {
      return db.missionLogs.logDebug(missionId, message, metadata);
    },
  );

  ipcMain.handle(
    "db:missionLogs:logAgentAction",
    (_event, missionId: string, action: string, details?: any) => {
      return db.missionLogs.logAgentAction(missionId, action, details);
    },
  );

  ipcMain.handle(
    "db:missionLogs:logUserInput",
    (_event, missionId: string, input: string) => {
      return db.missionLogs.logUserInput(missionId, input);
    },
  );

  ipcMain.handle("db:missionLogs:getStats", (_event, missionId: string) => {
    return db.missionLogs.getStats(missionId);
  });

  ipcMain.handle(
    "db:missionLogs:getUsageStats",
    (_event, missionId: string) => {
      return db.missionLogs.getUsageStats(missionId);
    },
  );

  ipcMain.handle(
    "db:missionLogs:getLatest",
    (_event, missionId: string, count?: number) => {
      return db.missionLogs.getLatest(missionId, count);
    },
  );

  // ==========================================
  // Database utility handlers
  // ==========================================
  ipcMain.handle("db:utils:backup", async (_event, destPath: string) => {
    const sourcePath = db.getPath();
    if (!sourcePath) throw new Error("Database not initialized");

    await fs.promises.copyFile(sourcePath, destPath);
    return true;
  });

  ipcMain.handle("db:utils:getPath", () => {
    return db.getPath();
  });

  ipcMain.handle("db:utils:getSize", async () => {
    const dbPath = db.getPath();
    if (!dbPath) return 0;

    try {
      const stats = await fs.promises.stat(dbPath);
      return stats.size;
    } catch {
      return 0;
    }
  });

  // ==========================================
  // AI Service handlers
  // ==========================================

  // Generate plan for a mission
  ipcMain.handle(
    "ai:generatePlan",
    async (_event, missionId: string, options?: { planFeedback?: string }) => {
      return aiOrchestrator.generatePlan(missionId, options);
    },
  );

  // Generate code for a mission
  ipcMain.handle(
    "ai:generateCode",
    async (_event, missionId: string, options?: { codeFeedback?: string }) => {
      return aiOrchestrator.generateCode(missionId, options);
    },
  );

  // Apply changes to the project
  ipcMain.handle(
    "ai:applyChanges",
    async (
      _event,
      missionId: string,
      options?: {
        createBackup?: boolean;
        dryRun?: boolean;
        filePaths?: string[];
        editedContent?: Record<string, string>;
      },
    ) => {
      return aiOrchestrator.applyChanges(missionId, options ?? {});
    },
  );

  // Test provider connection
  ipcMain.handle("ai:testConnection", async (_event, providerId: string) => {
    return aiOrchestrator.testProviderConnection(providerId);
  });

  // Validate provider configuration
  ipcMain.handle("ai:validateProvider", (_event, provider: any) => {
    return aiOrchestrator.validateProvider(provider);
  });

  // Invalidate adapter cache (when provider is updated)
  ipcMain.handle("ai:invalidateAdapter", (_event, providerId: string) => {
    aiOrchestrator.invalidateAdapter(providerId);
    return true;
  });

  // ==========================================
  // Git Service handlers
  // ==========================================

  // Get git info for a project path
  ipcMain.handle("git:getInfo", async (_event, projectPath: string) => {
    const gitService = new GitService(projectPath);
    return gitService.getGitInfo();
  });

  // Get git status
  ipcMain.handle("git:getStatus", async (_event, projectPath: string) => {
    const gitService = new GitService(projectPath);
    return gitService.getStatus();
  });

  ipcMain.handle("git:getBranchState", async (_event, projectPath: string) => {
    const gitService = new GitService(projectPath);
    return gitService.getBranchState();
  });

  // Get file diff vs HEAD (what will be committed after git add -A)
  ipcMain.handle(
    "git:getFileDiffHead",
    async (_event, projectPath: string, filePath: string) => {
      const gitService = new GitService(projectPath);
      return gitService.getFileDiffHead(filePath);
    },
  );

  ipcMain.handle(
    "git:getFileDiffAgainstBase",
    async (_event, projectPath: string, filePath: string, baseRef: string) => {
      const gitService = new GitService(projectPath);
      return gitService.getFileDiffAgainstBase(filePath, baseRef);
    },
  );

  // Check if directory is a git repo
  ipcMain.handle("git:isRepo", async (_event, projectPath: string) => {
    const gitService = new GitService(projectPath);
    return gitService.isGitRepo();
  });

  // Get current branch
  ipcMain.handle(
    "git:getCurrentBranch",
    async (_event, projectPath: string) => {
      const gitService = new GitService(projectPath);
      return gitService.getCurrentBranch();
    },
  );

  // Get default branch (main or master)
  ipcMain.handle(
    "git:getDefaultBranch",
    async (_event, projectPath: string) => {
      const gitService = new GitService(projectPath);
      return gitService.getDefaultBranch();
    },
  );

  // Create a new branch (optionally from a base branch)
  ipcMain.handle(
    "git:createBranch",
    async (
      _event,
      projectPath: string,
      branchName: string,
      fromBranch?: string,
    ) => {
      const gitService = new GitService(projectPath);
      return gitService.createBranch(branchName, fromBranch);
    },
  );

  // List tracked files
  ipcMain.handle(
    "git:listFiles",
    async (_event, projectPath: string, maxFiles?: number) => {
      const gitService = new GitService(projectPath);
      return gitService.listTrackedFiles(maxFiles);
    },
  );

  // Get recent commits
  ipcMain.handle(
    "git:getRecentCommits",
    async (_event, projectPath: string, count?: number) => {
      const gitService = new GitService(projectPath);
      return gitService.getRecentCommits(count);
    },
  );

  // Commit changes (optional file list; omit for git add -A)
  ipcMain.handle(
    "git:commit",
    async (_event, projectPath: string, message: string, files?: string[]) => {
      const gitService = new GitService(projectPath);
      return gitService.commit(message, files);
    },
  );

  // Get worktree info (isWorktree, worktreeRoot)
  ipcMain.handle("git:getWorktreeInfo", async (_event, projectPath: string) => {
    const gitService = new GitService(projectPath);
    return gitService.getWorktreeInfo();
  });

  // Push current branch to origin
  ipcMain.handle("git:push", async (_event, projectPath: string) => {
    const gitService = new GitService(projectPath);
    return gitService.push();
  });

  // Reset (discard changes or undo last commit)
  ipcMain.handle(
    "git:reset",
    async (_event, projectPath: string, ref?: "HEAD" | "HEAD~1") => {
      const gitService = new GitService(projectPath);
      return gitService.reset(ref ?? "HEAD");
    },
  );

  // ==========================================
  // Worktree (por missão, pipeline paralelo)
  // ==========================================
  ipcMain.handle(
    "worktree:ensureForMission",
    async (_event, missionId: string) => {
      const mission = db.missions.findById(missionId);
      if (!mission) return { success: false, error: "Mission not found" };
      const project = db.projects.findById(mission.projectId);
      if (!project) return { success: false, error: "Project not found" };
      const result = await createWorktreeForMission(
        project.path,
        missionId,
        mission.title,
      );
      if (!result.success) return result;
      db.missions.update(missionId, {
        worktreePath: result.data.worktreePath,
        worktreeBranch: result.data.worktreeBranch,
      });
      return {
        success: true,
        worktreePath: result.data.worktreePath,
        worktreeBranch: result.data.worktreeBranch,
      };
    },
  );

  ipcMain.handle(
    "worktree:mergeIntoMain",
    async (_event, missionId: string) => {
      const mission = db.missions.findById(missionId);
      if (!mission?.worktreePath || !mission?.worktreeBranch)
        return { success: false, error: "Mission has no worktree" };
      const project = db.projects.findById(mission.projectId);
      if (!project) return { success: false, error: "Project not found" };
      const gitService = new GitService(project.path);
      // Merge into the branch currently checked out in the project (e.g. main or ffeat/migrate-nextjs)
      let targetBranch = await gitService.getCurrentBranch();
      if (!targetBranch || targetBranch === "HEAD" || targetBranch === "unknown") {
        targetBranch = await gitService.getDefaultBranch().catch(() => "main");
      }
      const result = await mergeWorktreeIntoMain(
        project.path,
        mission.worktreeBranch,
        mission.worktreePath,
        targetBranch,
      );
      if (result.success)
        db.missions.update(missionId, {
          worktreePath: null,
          worktreeBranch: null,
        });
      return result;
    },
  );

  ipcMain.handle("worktree:discard", async (_event, missionId: string) => {
    const mission = db.missions.findById(missionId);
    if (!mission?.worktreePath || !mission?.worktreeBranch)
      return { success: false, error: "Mission has no worktree" };
    const project = db.projects.findById(mission.projectId);
    if (!project) return { success: false, error: "Project not found" };
    const result = await discardWorktree(
      project.path,
      mission.worktreePath,
      mission.worktreeBranch,
    );
    if (result.success) {
      db.missions.update(missionId, {
        worktreePath: null,
        worktreeBranch: null,
      });
      killPtyByMissionId(missionId);
    }
    return result;
  });

  // Escape string for use inside AppleScript do script "..." (backslash and double-quote)
  function escapeForAppleScriptDoScript(s: string): string {
    return s.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
  }

  /** Escape prompt for use inside double-quoted shell argument (darwin/linux: \ and "; win32: ") */
  function escapePromptForShell(
    plat: NodeJS.Platform,
    prompt: string,
  ): string {
    if (plat === "win32") {
      return prompt.replace(/"/g, '""');
    }
    return prompt.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
  }

  /** Build full command from cli + prompt; second arg can be string (opaque) or { cliCommand, prompt }. */
  function normalizeSuggestedCommand(
    plat: NodeJS.Platform,
    suggestedCommand: string | { cliCommand: string; prompt: string } | undefined,
  ): string | undefined {
    if (suggestedCommand == null) return undefined;
    if (typeof suggestedCommand === "string") return suggestedCommand;
    const { cliCommand, prompt } = suggestedCommand;
    const escaped = escapePromptForShell(plat, prompt);
    return `${cliCommand} "${escaped}"`;
  }

  // Abre o terminal do OS no path; opcionalmente sugere um comando (ex.: codex) ou { cliCommand, prompt }
  ipcMain.handle(
    "shell:openTerminalAtPath",
    async (
      _event,
      dirPath: string,
      suggestedCommand?: string | { cliCommand: string; prompt: string },
    ): Promise<{ success: boolean; error?: string }> => {
      const plat = platform();
      const fullCommand = normalizeSuggestedCommand(plat, suggestedCommand);
      try {
        if (plat === "darwin") {
          const pathEscaped = escapeForAppleScriptDoScript(dirPath);
          const cmdEscaped = fullCommand
            ? " && " + escapeForAppleScriptDoScript(fullCommand)
            : "";
          const script = `tell application "Terminal" to do script "cd \\"${pathEscaped}\\"${cmdEscaped}"`;
          execFileSync("osascript", ["-e", script]);
        } else if (plat === "win32") {
          const quoted = `"${dirPath.replace(/"/g, '""')}"`;
          const winCmd = fullCommand
            ? `start cmd /k "cd /d ${quoted} && ${fullCommand}"`
            : `start cmd /k "cd /d ${quoted}"`;
          execSync(winCmd, { windowsHide: true });
        } else {
          const term: string =
            process.env.COLORTERM || process.env.TERM || "xterm";
          const cmd = fullCommand
            ? `cd "${dirPath}" && ${fullCommand}`
            : `cd "${dirPath}"`;
          const sub = spawn(term, ["-e", `bash -c '${cmd.replace(/'/g, "'\"'\"'")}'`], {
            detached: true,
            stdio: "ignore",
          });
          sub.unref();
        }
        return { success: true };
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        return { success: false, error: message };
      }
    },
  );

  // Embedded terminal (node-pty + xterm.js)
  ipcMain.handle(
    "terminal:spawn",
    async (
      event,
      options: { cwd: string; command?: string; args?: string[]; cols?: number; rows?: number }
    ): Promise<{ ptyId?: string; error?: string }> => {
      const result = spawnPty(
        {
          cwd: options.cwd,
          command: options.command,
          args: options.args ?? [],
          cols: options.cols ?? 80,
          rows: options.rows ?? 24,
        },
        event.sender
      );
      if (result.error) return { error: result.error };
      return { ptyId: result.ptyId };
    }
  );

  ipcMain.handle(
    "terminal:getOrCreate",
    async (
      event,
      missionId: string,
      options: { cwd: string; command?: string; args?: string[]; cols?: number; rows?: number }
    ): Promise<{
      ptyId?: string;
      error?: string;
      session?: import("../lib/database/types").MissionAgentSession | null;
    }> => {
      const result = getOrCreatePty(
        {
          missionId,
          cwd: options.cwd,
          command: options.command,
          args: options.args ?? [],
          cols: options.cols ?? 80,
          rows: options.rows ?? 24,
        },
        event.sender
      );
      if (result.error) return { error: result.error };
      return { ptyId: result.ptyId, session: result.session ?? null };
    }
  );

  ipcMain.handle(
    "terminal:getSession",
    async (
      _event,
      missionId: string
    ): Promise<import("../lib/database/types").MissionAgentSession | null> => {
      return getMissionSession(missionId);
    }
  );

  ipcMain.handle(
    "terminal:write",
    (_event, ptyId: string, data: string): { ok: boolean } => {
      return { ok: writeToPty(ptyId, data) };
    }
  );

  ipcMain.handle(
    "terminal:resize",
    (_event, ptyId: string, cols: number, rows: number): { ok: boolean } => {
      return { ok: resizePty(ptyId, cols, rows) };
    }
  );

  ipcMain.handle(
    "terminal:kill",
    (_event, ptyId: string): { ok: boolean } => {
      return { ok: killPty(ptyId) };
    }
  );

  ipcMain.handle(
    "terminal:killByMissionId",
    (_event, missionId: string): { ok: boolean } => {
      return { ok: killPtyByMissionId(missionId) };
    }
  );

  console.log("[IPC] All handlers registered");
}
