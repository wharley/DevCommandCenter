"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
const electron_1 = require("electron");
// Type-safe IPC invoke wrapper
const invoke = (channel, ...args) => {
    return electron_1.ipcRenderer.invoke(channel, ...args);
};
// Expose protected methods to the renderer process
electron_1.contextBridge.exposeInMainWorld('electronAPI', {
    // App info
    platform: process.platform,
    // Dialog APIs
    dialog: {
        selectDirectory: () => invoke('dialog:selectDirectory'),
        showMessage: (options) => invoke('dialog:showMessage', options),
        confirm: (message) => invoke('dialog:confirm', message),
    },
    // Shell APIs
    shell: {
        openExternal: (url) => invoke('shell:openExternal', url),
        openPath: (path) => invoke('shell:openPath', path),
        showItemInFolder: (path) => invoke('shell:showItemInFolder', path),
    },
    // Window APIs
    window: {
        minimize: () => invoke('window:minimize'),
        maximize: () => invoke('window:maximize'),
        close: () => invoke('window:close'),
        isMaximized: () => invoke('window:isMaximized'),
    },
});
// Expose database API
electron_1.contextBridge.exposeInMainWorld('db', {
    // Providers
    providers: {
        findAll: () => invoke('db:providers:findAll'),
        findById: (id) => invoke('db:providers:findById', id),
        findByType: (type) => invoke('db:providers:findByType', type),
        findActive: () => invoke('db:providers:findActive'),
        create: (data) => invoke('db:providers:create', data),
        update: (id, data) => invoke('db:providers:update', id, data),
        delete: (id) => invoke('db:providers:delete', id),
        setActive: (id, isActive) => invoke('db:providers:setActive', id, isActive),
        testConnection: (id) => invoke('db:providers:testConnection', id),
    },
    // Projects
    projects: {
        findAll: () => invoke('db:projects:findAll'),
        findById: (id) => invoke('db:projects:findById', id),
        findByPath: (path) => invoke('db:projects:findByPath', path),
        search: (query) => invoke('db:projects:search', query),
        create: (data) => invoke('db:projects:create', data),
        update: (id, data) => invoke('db:projects:update', id, data),
        delete: (id) => invoke('db:projects:delete', id),
        getStats: (id) => invoke('db:projects:getStats', id),
        updateLastOpened: (id) => invoke('db:projects:updateLastOpened', id),
    },
    // Missions
    missions: {
        findAll: () => invoke('db:missions:findAll'),
        findById: (id) => invoke('db:missions:findById', id),
        findByProject: (projectId) => invoke('db:missions:findByProject', projectId),
        findByStatus: (status) => invoke('db:missions:findByStatus', status),
        findActive: () => invoke('db:missions:findActive'),
        search: (query, projectId) => invoke('db:missions:search', query, projectId),
        create: (data) => invoke('db:missions:create', data),
        update: (id, data) => invoke('db:missions:update', id, data),
        delete: (id) => invoke('db:missions:delete', id),
        updateStatus: (id, status) => invoke('db:missions:updateStatus', id, status),
        updatePlan: (id, plan) => invoke('db:missions:updatePlan', id, plan),
        updateGeneratedCode: (id, code) => invoke('db:missions:updateGeneratedCode', id, code),
        start: (id) => invoke('db:missions:start', id),
        complete: (id, summary) => invoke('db:missions:complete', id, summary),
        fail: (id, error) => invoke('db:missions:fail', id, error),
        cancel: (id) => invoke('db:missions:cancel', id),
        getFullMission: (id) => invoke('db:missions:getFullMission', id),
    },
    // Mission Logs
    missionLogs: {
        findAll: () => invoke('db:missionLogs:findAll'),
        findById: (id) => invoke('db:missionLogs:findById', id),
        findByMission: (missionId, limit, offset) => invoke('db:missionLogs:findByMission', missionId, limit, offset),
        findByLevel: (level, missionId) => invoke('db:missionLogs:findByLevel', level, missionId),
        search: (query, missionId) => invoke('db:missionLogs:search', query, missionId),
        create: (data) => invoke('db:missionLogs:create', data),
        delete: (id) => invoke('db:missionLogs:delete', id),
        deleteByMission: (missionId) => invoke('db:missionLogs:deleteByMission', missionId),
        logInfo: (missionId, message, metadata) => invoke('db:missionLogs:logInfo', missionId, message, metadata),
        logWarning: (missionId, message, metadata) => invoke('db:missionLogs:logWarning', missionId, message, metadata),
        logError: (missionId, message, metadata) => invoke('db:missionLogs:logError', missionId, message, metadata),
        logDebug: (missionId, message, metadata) => invoke('db:missionLogs:logDebug', missionId, message, metadata),
        logAgentAction: (missionId, action, details) => invoke('db:missionLogs:logAgentAction', missionId, action, details),
        logUserInput: (missionId, input) => invoke('db:missionLogs:logUserInput', missionId, input),
        getStats: (missionId) => invoke('db:missionLogs:getStats', missionId),
        getLatest: (missionId, count) => invoke('db:missionLogs:getLatest', missionId, count),
    },
    // Database utilities
    utils: {
        backup: (destPath) => invoke('db:utils:backup', destPath),
        getPath: () => invoke('db:utils:getPath'),
        getSize: () => invoke('db:utils:getSize'),
    },
});
// Notify renderer that preload script is ready
window.addEventListener('DOMContentLoaded', () => {
    console.log('[Preload] DOM content loaded, Electron APIs exposed');
});
//# sourceMappingURL=preload.js.map