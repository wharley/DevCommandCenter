import { contextBridge, ipcRenderer } from "electron";

// Type-safe IPC invoke wrapper
const invoke = (channel: string, ...args: unknown[]): Promise<any> => {
  return ipcRenderer.invoke(channel, ...args);
};

// Expose protected methods to the renderer process
contextBridge.exposeInMainWorld("electronAPI", {
  // App info
  platform: process.platform,

  // Dialog APIs
  dialog: {
    selectDirectory: () => invoke("dialog:selectDirectory"),
    showMessage: (options: { type: string; title: string; message: string }) =>
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
  },

  // Window APIs
  window: {
    minimize: () => invoke("window:minimize"),
    maximize: () => invoke("window:maximize"),
    close: () => invoke("window:close"),
    isMaximized: () => invoke("window:isMaximized"),
  },

  // AI Service APIs
  ai: {
    generatePlan: (missionId: string) => invoke("ai:generatePlan", missionId),
    generateCode: (missionId: string) => invoke("ai:generateCode", missionId),
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
    getFileDiffHead: (projectPath: string, filePath: string) =>
      invoke("git:getFileDiffHead", projectPath, filePath),
    isRepo: (projectPath: string) => invoke("git:isRepo", projectPath),
    getCurrentBranch: (projectPath: string) =>
      invoke("git:getCurrentBranch", projectPath),
    getDefaultBranch: (projectPath: string) =>
      invoke("git:getDefaultBranch", projectPath),
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
    getWorktreeInfo: (projectPath: string) =>
      invoke("git:getWorktreeInfo", projectPath),
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
