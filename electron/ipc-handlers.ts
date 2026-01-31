import { dialog, shell, BrowserWindow } from "electron";
import type { IpcMain } from "electron";
import fs from "node:fs";
import { execSync } from "node:child_process";
import { platform } from "node:os";
import db from "../lib/database";
import { aiOrchestrator, GitService } from "./services";
import {
  providerService,
  sanitizeForRenderer,
} from "./services/provider-service";

export function registerIpcHandlers(ipcMain: IpcMain) {
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

  ipcMain.handle("db:missionLogs:getUsageStats", (_event, missionId: string) => {
    return db.missionLogs.getUsageStats(missionId);
  });

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
  ipcMain.handle("ai:generatePlan", async (_event, missionId: string) => {
    return aiOrchestrator.generatePlan(missionId);
  });

  // Generate code for a mission
  ipcMain.handle("ai:generateCode", async (_event, missionId: string) => {
    return aiOrchestrator.generateCode(missionId);
  });

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

  // Get file diff vs HEAD (what will be committed after git add -A)
  ipcMain.handle(
    "git:getFileDiffHead",
    async (_event, projectPath: string, filePath: string) => {
      const gitService = new GitService(projectPath);
      return gitService.getFileDiffHead(filePath);
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
    async (
      _event,
      projectPath: string,
      message: string,
      files?: string[],
    ) => {
      const gitService = new GitService(projectPath);
      return gitService.commit(message, files);
    },
  );

  // Get worktree info (isWorktree, worktreeRoot)
  ipcMain.handle(
    "git:getWorktreeInfo",
    async (_event, projectPath: string) => {
      const gitService = new GitService(projectPath);
      return gitService.getWorktreeInfo();
    },
  );

  // Push current branch to origin
  ipcMain.handle("git:push", async (_event, projectPath: string) => {
    const gitService = new GitService(projectPath);
    return gitService.push();
  });

  console.log("[IPC] All handlers registered");
}
