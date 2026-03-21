import { contextBridge, ipcRenderer } from "electron";

// Type-safe IPC invoke wrapper
const invoke = (channel: string, ...args: unknown[]): Promise<any> => {
  return ipcRenderer.invoke(channel, ...args);
};

// Expose protected methods to the renderer process
contextBridge.exposeInMainWorld("electronAPI", {
  // App info
  platform: process.platform,

  // App update (version, check, install, status events)
  app: {
    getVersion: () => invoke("app:getVersion") as Promise<string>,
    checkForUpdates: () => invoke("app:checkForUpdates") as Promise<void>,
    quitAndInstall: () => invoke("app:quitAndInstall") as Promise<void>,
    showNotification: (payload: { title: string; body?: string }) =>
      invoke("app:showNotification", payload),
    onUpdateStatus: (
      callback: (payload: {
        type: "available" | "not-available" | "downloaded" | "error";
        version?: string;
        message?: string;
      }) => void
    ) => {
      const fn = (_: unknown, payload: Parameters<typeof callback>[0]) =>
        callback(payload);
      ipcRenderer.on("app:update-status", fn);
      return () => {
        ipcRenderer.removeListener("app:update-status", fn);
      };
    },
  },

  // Dialog APIs
  dialog: {
    selectDirectory: () => invoke("dialog:selectDirectory"),
    showMessage: (options: {
      type: string;
      title: string;
      message: string;
      detail?: string;
      buttons?: string[];
      defaultId?: number;
      cancelId?: number;
    }) =>
      invoke("dialog:showMessage", options),
    confirm: (message: string) => invoke("dialog:confirm", message),
  },

  // Shell APIs
  shell: {
    openExternal: (url: string) => invoke("shell:openExternal", url),
    openPath: (path: string) => invoke("shell:openPath", path),
    showItemInFolder: (path: string) => invoke("shell:showItemInFolder", path),
    resolveCliPath: (command: string) =>
      invoke("shell:resolveCliPath", command),
    detectCliForProvider: (providerType: string) =>
      invoke("shell:detectCliForProvider", providerType),
    validateCliPath: (providerType: string, cliPath: string) =>
      invoke("shell:validateCliPath", providerType, cliPath),
    openTerminalAtPath: (
      dirPath: string,
      suggestedCommand?: string | { cliCommand: string; prompt: string },
    ) => invoke("shell:openTerminalAtPath", dirPath, suggestedCommand),
  },

  terminal: {
    spawn: (options: {
      cwd: string;
      command?: string;
      args?: string[];
      cols?: number;
      rows?: number;
    }) =>
      invoke("terminal:spawn", options) as Promise<{
        ptyId?: string;
        error?: string;
      }>,
    getOrCreate: (missionId: string, options: {
      cwd: string;
      command?: string;
      args?: string[];
      cols?: number;
      rows?: number;
    }) =>
      invoke("terminal:getOrCreate", missionId, options) as Promise<{
        ptyId?: string;
        error?: string;
        session?: import("../lib/database/types").MissionAgentSession | null;
      }>,
    getSession: (missionId: string) =>
      invoke("terminal:getSession", missionId) as Promise<
        import("../lib/database/types").MissionAgentSession | null
      >,
    write: (ptyId: string, data: string) =>
      invoke("terminal:write", ptyId, data) as Promise<{ ok: boolean }>,
    resize: (ptyId: string, cols: number, rows: number) =>
      invoke("terminal:resize", ptyId, cols, rows) as Promise<{ ok: boolean }>,
    kill: (ptyId: string) =>
      invoke("terminal:kill", ptyId) as Promise<{ ok: boolean }>,
    killByMissionId: (missionId: string) =>
      invoke("terminal:killByMissionId", missionId) as Promise<{ ok: boolean }>,
    getOrCreateForPane: (paneId: string, options: {
      cwd: string;
      command?: string;
      args?: string[];
      cols?: number;
      rows?: number;
      restart?: boolean;
    }) =>
      invoke("terminal:getOrCreateForPane", paneId, options) as Promise<{
        ptyId?: string;
        error?: string;
        session?: import("../lib/database/types").PaneSession | null;
      }>,
    getPaneSession: (paneId: string) =>
      invoke("terminal:getPaneSession", paneId) as Promise<
        import("../lib/database/types").PaneSession | null
      >,
    killByPaneId: (paneId: string) =>
      invoke("terminal:killByPaneId", paneId) as Promise<{ ok: boolean }>,
    getProjectActivity: (projectId: string) =>
      invoke("terminal:getProjectActivity", projectId) as Promise<{
        totalRunningPanes: number;
        runningPanesByCombId: Record<string, number>;
      }>,
    onData: (callback: (ptyId: string, data: string) => void) => {
      const fn = (_: unknown, ptyId: string, data: string) =>
        callback(ptyId, data);
      ipcRenderer.on("terminal:data", fn);
      return () => ipcRenderer.removeListener("terminal:data", fn);
    },
    onExit: (callback: (ptyId: string, code: number) => void) => {
      const fn = (_: unknown, ptyId: string, code: number) =>
        callback(ptyId, code);
      ipcRenderer.on("terminal:exit", fn);
      return () => ipcRenderer.removeListener("terminal:exit", fn);
    },
    onAttention: (
      callback: (payload: import("../lib/terminal/attention-types").TerminalAttentionPayload) => void,
    ) => {
      const fn = (
        _: unknown,
        payload: import("../lib/terminal/attention-types").TerminalAttentionPayload,
      ) => callback(payload);
      ipcRenderer.on("terminal:attention", fn);
      return () => ipcRenderer.removeListener("terminal:attention", fn);
    },
  },

  worktree: {
    ensureForMission: (missionId: string) =>
      invoke("worktree:ensureForMission", missionId),
    mergeIntoMain: (missionId: string) =>
      invoke("worktree:mergeIntoMain", missionId),
    discard: (missionId: string) => invoke("worktree:discard", missionId),
    getDiffs: (missionId: string) =>
      invoke("missions:getDiffs", missionId),
    applyMissionPatch: (
      missionId: string,
      targetBranch: string,
      options?: { includeFiles?: string[]; commit?: boolean; message?: string },
    ) =>
      invoke("worktree:applyMissionPatch", missionId, targetBranch, options),
  },

  comb: {
    ensureWorktree: (combId: string) =>
      invoke("comb:ensureWorktree", combId) as Promise<{
        success: boolean;
        error?: string;
        worktreePath?: string;
        branch?: string;
      }>,
    discard: (combId: string) =>
      invoke("comb:discard", combId) as Promise<{ success: boolean; error?: string }>,
    mergeIntoMain: (combId: string, targetBranch?: string) =>
      invoke("comb:mergeIntoMain", combId, targetBranch) as Promise<{
        success: boolean;
        error?: string;
      }>,
    getDiffs: (combId: string) =>
      invoke("comb:getDiffs", combId) as Promise<{
        success: boolean;
        error?: string;
        files: Array<{ path: string; status: string; diff: string }>;
        summary: { changedFiles: number; insertions: number; deletions: number } | null;
      }>,
    applyPatch: (
      combId: string,
      targetBranch: string,
      options?: { includeFiles?: string[]; commit?: boolean; message?: string },
    ) =>
      invoke("comb:applyPatch", combId, targetBranch, options) as Promise<{
        success: boolean;
        error?: string;
      }>,
  },

  // Window APIs
  window: {
    minimize: () => invoke("window:minimize"),
    maximize: () => invoke("window:maximize"),
    close: () => invoke("window:close"),
    isMaximized: () => invoke("window:isMaximized"),
  },

  // License / activation (beta)
  license: {
    getStatus: () =>
      invoke("license:getStatus") as Promise<{
        activated: boolean;
        email?: string;
        activatedAt?: string;
      }>,
    getMachineId: () => invoke("license:getMachineId") as Promise<string>,
    activate: (email: string) =>
      invoke("license:activate", email) as Promise<{
        success: boolean;
        message?: string;
      }>,
    skipActivation: () =>
      invoke("license:skipActivation") as Promise<{ success: boolean }>,
  },

  // AI Service APIs
  ai: {
    generatePlan: (missionId: string, options?: { planFeedback?: string }) =>
      invoke("ai:generatePlan", missionId, options),
    generateCode: (missionId: string, options?: { codeFeedback?: string }) =>
      invoke("ai:generateCode", missionId, options),
    applyChanges: (
      missionId: string,
      options?: {
        createBackup?: boolean;
        dryRun?: boolean;
        filePaths?: string[];
        editedContent?: Record<string, string>;
      },
    ) => invoke("ai:applyChanges", missionId, options),
    testConnection: (providerId: string) =>
      invoke("ai:testConnection", providerId),
    validateProvider: (provider: unknown) =>
      invoke("ai:validateProvider", provider),
    invalidateAdapter: (providerId: string) =>
      invoke("ai:invalidateAdapter", providerId),
  },

  // Git APIs
  git: {
    getInfo: (projectPath: string) => invoke("git:getInfo", projectPath),
    getStatus: (projectPath: string) => invoke("git:getStatus", projectPath),
    getBranchState: (projectPath: string) =>
      invoke("git:getBranchState", projectPath),
    getFileDiffHead: (projectPath: string, filePath: string) =>
      invoke("git:getFileDiffHead", projectPath, filePath),
    getFileDiffAgainstBase: (
      projectPath: string,
      filePath: string,
      baseRef: string,
    ) => invoke("git:getFileDiffAgainstBase", projectPath, filePath, baseRef),
    isRepo: (projectPath: string) => invoke("git:isRepo", projectPath),
    getCurrentBranch: (projectPath: string) =>
      invoke("git:getCurrentBranch", projectPath),
    getDefaultBranch: (projectPath: string) =>
      invoke("git:getDefaultBranch", projectPath),
    getLocalBranches: (projectPath: string) =>
      invoke("git:getLocalBranches", projectPath),
    createBranch: (
      projectPath: string,
      branchName: string,
      fromBranch?: string,
    ) => invoke("git:createBranch", projectPath, branchName, fromBranch),
    listFiles: (projectPath: string, maxFiles?: number) =>
      invoke("git:listFiles", projectPath, maxFiles),
    getRecentCommits: (projectPath: string, count?: number) =>
      invoke("git:getRecentCommits", projectPath, count),
    commit: (projectPath: string, message: string, files?: string[]) =>
      invoke("git:commit", projectPath, message, files),
    push: (projectPath: string) => invoke("git:push", projectPath),
    pull: (projectPath: string) => invoke("git:pull", projectPath),
    reset: (
      projectPath: string,
      ref?: "HEAD" | "HEAD~1",
    ) => invoke("git:reset", projectPath, ref ?? "HEAD"),
    getWorktreeInfo: (projectPath: string) =>
      invoke("git:getWorktreeInfo", projectPath),
    getReviewDiffs: (worktreePath: string) =>
      invoke("git:getReviewDiffs", worktreePath) as Promise<{
        success: boolean;
        error?: string;
        files: Array<{ path: string; status: string; diff: string }>;
        summary: {
          changedFiles: number;
          insertions: number;
          deletions: number;
        } | null;
      }>,
    applyWorktreePatch: (
      mainProjectPath: string,
      worktreePath: string,
      targetBranch: string,
      options?: {
        includeFiles?: string[];
        commit?: boolean;
        message?: string;
      },
    ) =>
      invoke(
        "git:applyWorktreePatch",
        mainProjectPath,
        worktreePath,
        targetBranch,
        options,
      ) as Promise<{ success: boolean; error?: string; applyFailed?: boolean }>,
  },

  review: {
    getDiffsBundle: (worktreePaths: string[]) =>
      invoke("review:getDiffsBundle", worktreePaths) as Promise<
        Array<{
          worktreePath: string;
          success: boolean;
          error?: string;
          files: Array<{ path: string; status: string; diff: string }>;
          summary: {
            changedFiles: number;
            insertions: number;
            deletions: number;
          } | null;
        }>
      >,
  },
});

// Expose database API
contextBridge.exposeInMainWorld("db", {
  // Providers
  providers: {
    findAll: () => invoke("db:providers:findAll"),
    findById: (id: string) => invoke("db:providers:findById", id),
    findByType: (type: string) => invoke("db:providers:findByType", type),
    findActive: () => invoke("db:providers:findActive"),
    create: (data: unknown) => invoke("db:providers:create", data),
    update: (id: string, data: unknown) =>
      invoke("db:providers:update", id, data),
    delete: (id: string) => invoke("db:providers:delete", id),
    setActive: (id: string, isActive: boolean) =>
      invoke("db:providers:setActive", id, isActive),
    testConnection: (id: string) => invoke("db:providers:testConnection", id),
    isEncryptionAvailable: () =>
      invoke("db:providers:isEncryptionAvailable"),
  },

  // Projects
  projects: {
    findAll: () => invoke("db:projects:findAll"),
    findById: (id: string) => invoke("db:projects:findById", id),
    findByPath: (path: string) => invoke("db:projects:findByPath", path),
    search: (query: string) => invoke("db:projects:search", query),
    create: (data: unknown) => invoke("db:projects:create", data),
    update: (id: string, data: unknown) =>
      invoke("db:projects:update", id, data),
    delete: (id: string) => invoke("db:projects:delete", id),
    getStats: (id: string) => invoke("db:projects:getStats", id),
    updateLastOpened: (id: string) =>
      invoke("db:projects:updateLastOpened", id),
  },

  // Missions
  missions: {
    findAll: () => invoke("db:missions:findAll"),
    findById: (id: string) => invoke("db:missions:findById", id),
    findByProject: (projectId: string) =>
      invoke("db:missions:findByProject", projectId),
    findByStatus: (status: string) =>
      invoke("db:missions:findByStatus", status),
    findActive: () => invoke("db:missions:findActive"),
    search: (query: string, projectId?: string) =>
      invoke("db:missions:search", query, projectId),
    create: (data: unknown) => invoke("db:missions:create", data),
    update: (id: string, data: unknown) =>
      invoke("db:missions:update", id, data),
    delete: (id: string) => invoke("db:missions:delete", id),
    updateStatus: (id: string, status: string) =>
      invoke("db:missions:updateStatus", id, status),
    updatePlan: (id: string, plan: string) =>
      invoke("db:missions:updatePlan", id, plan),
    updateGeneratedCode: (id: string, code: string) =>
      invoke("db:missions:updateGeneratedCode", id, code),
    start: (id: string) => invoke("db:missions:start", id),
    complete: (id: string, summary?: string) =>
      invoke("db:missions:complete", id, summary),
    fail: (id: string, error: string) => invoke("db:missions:fail", id, error),
    cancel: (id: string) => invoke("db:missions:cancel", id),
    getFullMission: (id: string) => invoke("db:missions:getFullMission", id),
  },

  // Mission Logs
  missionLogs: {
    findAll: () => invoke("db:missionLogs:findAll"),
    findById: (id: string) => invoke("db:missionLogs:findById", id),
    findByMission: (missionId: string, limit?: number, offset?: number) =>
      invoke("db:missionLogs:findByMission", missionId, limit, offset),
    findByLevel: (level: string, missionId?: string) =>
      invoke("db:missionLogs:findByLevel", level, missionId),
    search: (query: string, missionId?: string) =>
      invoke("db:missionLogs:search", query, missionId),
    create: (data: unknown) => invoke("db:missionLogs:create", data),
    delete: (id: string) => invoke("db:missionLogs:delete", id),
    deleteByMission: (missionId: string) =>
      invoke("db:missionLogs:deleteByMission", missionId),
    logInfo: (missionId: string, message: string, metadata?: unknown) =>
      invoke("db:missionLogs:logInfo", missionId, message, metadata),
    logWarning: (missionId: string, message: string, metadata?: unknown) =>
      invoke("db:missionLogs:logWarning", missionId, message, metadata),
    logError: (missionId: string, message: string, metadata?: unknown) =>
      invoke("db:missionLogs:logError", missionId, message, metadata),
    logDebug: (missionId: string, message: string, metadata?: unknown) =>
      invoke("db:missionLogs:logDebug", missionId, message, metadata),
    logAgentAction: (missionId: string, action: string, details?: unknown) =>
      invoke("db:missionLogs:logAgentAction", missionId, action, details),
    logUserInput: (missionId: string, input: string) =>
      invoke("db:missionLogs:logUserInput", missionId, input),
    getStats: (missionId: string) =>
      invoke("db:missionLogs:getStats", missionId),
    getUsageStats: (missionId: string) =>
      invoke("db:missionLogs:getUsageStats", missionId),
    getLatest: (missionId: string, count?: number) =>
      invoke("db:missionLogs:getLatest", missionId, count),
  },

  // Combs
  combs: {
    findByProject: (projectId: string) =>
      invoke("db:combs:findByProject", projectId),
    findById: (id: string) => invoke("db:combs:findById", id),
    create: (data: unknown) => invoke("db:combs:create", data),
    update: (id: string, data: unknown) =>
      invoke("db:combs:update", id, data),
    delete: (id: string) => invoke("db:combs:delete", id),
  },

  // Panes
  panes: {
    findByComb: (combId: string) => invoke("db:panes:findByComb", combId),
    findById: (id: string) => invoke("db:panes:findById", id),
    create: (data: unknown) => invoke("db:panes:create", data),
    update: (id: string, data: unknown) =>
      invoke("db:panes:update", id, data),
    delete: (id: string) => invoke("db:panes:delete", id),
  },

  // Database utilities
  utils: {
    backup: (destPath: string) => invoke("db:utils:backup", destPath),
    getPath: () => invoke("db:utils:getPath"),
    getSize: () => invoke("db:utils:getSize"),
  },
});

// Notify renderer that preload script is ready
window.addEventListener("DOMContentLoaded", () => {
  console.log("[Preload] DOM content loaded, Electron APIs exposed");
  console.log("[Preload] Platform:", process.platform);
  console.log("[Preload] Electron version:", process.versions.electron);
});
