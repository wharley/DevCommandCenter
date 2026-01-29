"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.registerIpcHandlers = registerIpcHandlers;
const electron_1 = require("electron");
const node_fs_1 = __importDefault(require("node:fs"));
const database_1 = __importDefault(require("../lib/database"));
function registerIpcHandlers(ipcMain) {
    // ==========================================
    // Dialog handlers
    // ==========================================
    ipcMain.handle('dialog:selectDirectory', async () => {
        const result = await electron_1.dialog.showOpenDialog({
            properties: ['openDirectory', 'createDirectory'],
            title: 'Select Project Directory',
        });
        return result.canceled ? null : result.filePaths[0];
    });
    ipcMain.handle('dialog:showMessage', async (_event, options) => {
        const result = await electron_1.dialog.showMessageBox(options);
        return result.response;
    });
    ipcMain.handle('dialog:confirm', async (_event, message) => {
        const result = await electron_1.dialog.showMessageBox({
            type: 'question',
            buttons: ['Cancel', 'Confirm'],
            defaultId: 1,
            cancelId: 0,
            message,
        });
        return result.response === 1;
    });
    // ==========================================
    // Shell handlers
    // ==========================================
    ipcMain.handle('shell:openExternal', async (_event, url) => {
        await electron_1.shell.openExternal(url);
    });
    ipcMain.handle('shell:openPath', async (_event, path) => {
        await electron_1.shell.openPath(path);
    });
    ipcMain.handle('shell:showItemInFolder', (_event, path) => {
        electron_1.shell.showItemInFolder(path);
    });
    // ==========================================
    // Window handlers
    // ==========================================
    ipcMain.handle('window:minimize', (event) => {
        electron_1.BrowserWindow.fromWebContents(event.sender)?.minimize();
    });
    ipcMain.handle('window:maximize', (event) => {
        const win = electron_1.BrowserWindow.fromWebContents(event.sender);
        if (win?.isMaximized()) {
            win.unmaximize();
        }
        else {
            win?.maximize();
        }
    });
    ipcMain.handle('window:close', (event) => {
        electron_1.BrowserWindow.fromWebContents(event.sender)?.close();
    });
    ipcMain.handle('window:isMaximized', (event) => {
        return electron_1.BrowserWindow.fromWebContents(event.sender)?.isMaximized() ?? false;
    });
    // ==========================================
    // Provider handlers
    // ==========================================
    ipcMain.handle('db:providers:findAll', () => {
        return database_1.default.providers.findAll();
    });
    ipcMain.handle('db:providers:findById', (_event, id) => {
        return database_1.default.providers.findById(id);
    });
    ipcMain.handle('db:providers:findByType', (_event, type) => {
        return database_1.default.providers.findByType(type);
    });
    ipcMain.handle('db:providers:findActive', () => {
        return database_1.default.providers.findActive();
    });
    ipcMain.handle('db:providers:create', (_event, data) => {
        return database_1.default.providers.create(data);
    });
    ipcMain.handle('db:providers:update', (_event, id, data) => {
        return database_1.default.providers.update(id, data);
    });
    ipcMain.handle('db:providers:delete', (_event, id) => {
        return database_1.default.providers.delete(id);
    });
    ipcMain.handle('db:providers:setActive', (_event, id, isActive) => {
        return database_1.default.providers.setActive(id, isActive);
    });
    ipcMain.handle('db:providers:testConnection', (_event, id) => {
        return database_1.default.providers.testConnection(id);
    });
    // ==========================================
    // Project handlers
    // ==========================================
    ipcMain.handle('db:projects:findAll', () => {
        return database_1.default.projects.findAll();
    });
    ipcMain.handle('db:projects:findById', (_event, id) => {
        return database_1.default.projects.findById(id);
    });
    ipcMain.handle('db:projects:findByPath', (_event, path) => {
        return database_1.default.projects.findByPath(path);
    });
    ipcMain.handle('db:projects:search', (_event, query) => {
        return database_1.default.projects.search(query);
    });
    ipcMain.handle('db:projects:create', (_event, data) => {
        return database_1.default.projects.create(data);
    });
    ipcMain.handle('db:projects:update', (_event, id, data) => {
        return database_1.default.projects.update(id, data);
    });
    ipcMain.handle('db:projects:delete', (_event, id) => {
        return database_1.default.projects.delete(id);
    });
    ipcMain.handle('db:projects:getStats', (_event, id) => {
        return database_1.default.projects.getStats(id);
    });
    ipcMain.handle('db:projects:updateLastOpened', (_event, id) => {
        return database_1.default.projects.updateLastOpened(id);
    });
    // ==========================================
    // Mission handlers
    // ==========================================
    ipcMain.handle('db:missions:findAll', () => {
        return database_1.default.missions.findAll();
    });
    ipcMain.handle('db:missions:findById', (_event, id) => {
        return database_1.default.missions.findById(id);
    });
    ipcMain.handle('db:missions:findByProject', (_event, projectId) => {
        return database_1.default.missions.findByProject(projectId);
    });
    ipcMain.handle('db:missions:findByStatus', (_event, status) => {
        return database_1.default.missions.findByStatus(status);
    });
    ipcMain.handle('db:missions:findActive', () => {
        return database_1.default.missions.findActive();
    });
    ipcMain.handle('db:missions:search', (_event, query, projectId) => {
        return database_1.default.missions.search(query, projectId);
    });
    ipcMain.handle('db:missions:create', (_event, data) => {
        return database_1.default.missions.create(data);
    });
    ipcMain.handle('db:missions:update', (_event, id, data) => {
        return database_1.default.missions.update(id, data);
    });
    ipcMain.handle('db:missions:delete', (_event, id) => {
        return database_1.default.missions.delete(id);
    });
    ipcMain.handle('db:missions:updateStatus', (_event, id, status) => {
        return database_1.default.missions.updateStatus(id, status);
    });
    ipcMain.handle('db:missions:updatePlan', (_event, id, plan) => {
        return database_1.default.missions.updatePlan(id, plan);
    });
    ipcMain.handle('db:missions:updateGeneratedCode', (_event, id, code) => {
        return database_1.default.missions.updateGeneratedCode(id, code);
    });
    ipcMain.handle('db:missions:start', (_event, id) => {
        return database_1.default.missions.start(id);
    });
    ipcMain.handle('db:missions:complete', (_event, id, summary) => {
        return database_1.default.missions.complete(id, summary);
    });
    ipcMain.handle('db:missions:fail', (_event, id, error) => {
        return database_1.default.missions.fail(id, error);
    });
    ipcMain.handle('db:missions:cancel', (_event, id) => {
        return database_1.default.missions.cancel(id);
    });
    ipcMain.handle('db:missions:getFullMission', (_event, id) => {
        return database_1.default.missions.getFullMission(id);
    });
    // ==========================================
    // Mission Log handlers
    // ==========================================
    ipcMain.handle('db:missionLogs:findAll', () => {
        return database_1.default.missionLogs.findAll();
    });
    ipcMain.handle('db:missionLogs:findById', (_event, id) => {
        return database_1.default.missionLogs.findById(id);
    });
    ipcMain.handle('db:missionLogs:findByMission', (_event, missionId, limit, offset) => {
        return database_1.default.missionLogs.findByMission(missionId, limit, offset);
    });
    ipcMain.handle('db:missionLogs:findByLevel', (_event, level, missionId) => {
        return database_1.default.missionLogs.findByLevel(level, missionId);
    });
    ipcMain.handle('db:missionLogs:search', (_event, query, missionId) => {
        return database_1.default.missionLogs.search(query, missionId);
    });
    ipcMain.handle('db:missionLogs:create', (_event, data) => {
        return database_1.default.missionLogs.create(data);
    });
    ipcMain.handle('db:missionLogs:delete', (_event, id) => {
        return database_1.default.missionLogs.delete(id);
    });
    ipcMain.handle('db:missionLogs:deleteByMission', (_event, missionId) => {
        return database_1.default.missionLogs.deleteByMission(missionId);
    });
    ipcMain.handle('db:missionLogs:logInfo', (_event, missionId, message, metadata) => {
        return database_1.default.missionLogs.logInfo(missionId, message, metadata);
    });
    ipcMain.handle('db:missionLogs:logWarning', (_event, missionId, message, metadata) => {
        return database_1.default.missionLogs.logWarning(missionId, message, metadata);
    });
    ipcMain.handle('db:missionLogs:logError', (_event, missionId, message, metadata) => {
        return database_1.default.missionLogs.logError(missionId, message, metadata);
    });
    ipcMain.handle('db:missionLogs:logDebug', (_event, missionId, message, metadata) => {
        return database_1.default.missionLogs.logDebug(missionId, message, metadata);
    });
    ipcMain.handle('db:missionLogs:logAgentAction', (_event, missionId, action, details) => {
        return database_1.default.missionLogs.logAgentAction(missionId, action, details);
    });
    ipcMain.handle('db:missionLogs:logUserInput', (_event, missionId, input) => {
        return database_1.default.missionLogs.logUserInput(missionId, input);
    });
    ipcMain.handle('db:missionLogs:getStats', (_event, missionId) => {
        return database_1.default.missionLogs.getStats(missionId);
    });
    ipcMain.handle('db:missionLogs:getLatest', (_event, missionId, count) => {
        return database_1.default.missionLogs.getLatest(missionId, count);
    });
    // ==========================================
    // Database utility handlers
    // ==========================================
    ipcMain.handle('db:utils:backup', async (_event, destPath) => {
        const sourcePath = database_1.default.getPath();
        if (!sourcePath)
            throw new Error('Database not initialized');
        await node_fs_1.default.promises.copyFile(sourcePath, destPath);
        return true;
    });
    ipcMain.handle('db:utils:getPath', () => {
        return database_1.default.getPath();
    });
    ipcMain.handle('db:utils:getSize', async () => {
        const dbPath = database_1.default.getPath();
        if (!dbPath)
            return 0;
        try {
            const stats = await node_fs_1.default.promises.stat(dbPath);
            return stats.size;
        }
        catch {
            return 0;
        }
    });
    console.log('[IPC] All handlers registered');
}
//# sourceMappingURL=ipc-handlers.js.map