"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const electron_1 = require("electron");
const node_path_1 = __importDefault(require("node:path"));
const ipc_handlers_1 = require("./ipc-handlers");
const database_1 = __importDefault(require("../lib/database"));
// Handle creating/removing shortcuts on Windows when installing/uninstalling.
if (require('electron-squirrel-startup')) {
    electron_1.app.quit();
}
let mainWindow = null;
const isDev = process.env.NODE_ENV === 'development';
function createWindow() {
    mainWindow = new electron_1.BrowserWindow({
        width: 1400,
        height: 900,
        minWidth: 1000,
        minHeight: 700,
        titleBarStyle: 'hiddenInset',
        trafficLightPosition: { x: 16, y: 16 },
        backgroundColor: '#0a0a0f',
        webPreferences: {
            preload: node_path_1.default.join(__dirname, 'preload.js'),
            contextIsolation: true,
            nodeIntegration: false,
            sandbox: false,
        },
    });
    // Load the app
    if (isDev) {
        mainWindow.loadURL('http://localhost:3000');
        mainWindow.webContents.openDevTools();
    }
    else {
        mainWindow.loadFile(node_path_1.default.join(__dirname, '../out/index.html'));
    }
    mainWindow.on('closed', () => {
        mainWindow = null;
    });
    // Open external links in default browser
    mainWindow.webContents.setWindowOpenHandler(({ url }) => {
        electron_1.shell.openExternal(url);
        return { action: 'deny' };
    });
}
// Initialize database and register IPC handlers
function initializeApp() {
    try {
        database_1.default.init();
        console.log('[Electron] Database initialized successfully');
    }
    catch (error) {
        console.error('[Electron] Failed to initialize database:', error);
        electron_1.dialog.showErrorBox('Database Error', 'Failed to initialize the database. The app may not work correctly.');
    }
    (0, ipc_handlers_1.registerIpcHandlers)(electron_1.ipcMain);
}
// App lifecycle
electron_1.app.whenReady().then(() => {
    initializeApp();
    createWindow();
    electron_1.app.on('activate', () => {
        if (electron_1.BrowserWindow.getAllWindows().length === 0) {
            createWindow();
        }
    });
});
electron_1.app.on('window-all-closed', () => {
    if (process.platform !== 'darwin') {
        electron_1.app.quit();
    }
});
electron_1.app.on('before-quit', () => {
    try {
        database_1.default.close();
        console.log('[Electron] Database closed successfully');
    }
    catch (error) {
        console.error('[Electron] Error closing database:', error);
    }
});
// Handle certificate errors in development
if (isDev) {
    electron_1.app.on('certificate-error', (event, _webContents, _url, _error, _certificate, callback) => {
        event.preventDefault();
        callback(true);
    });
}
// Security: Prevent new window creation
electron_1.app.on('web-contents-created', (_event, contents) => {
    contents.on('will-navigate', (event, navigationUrl) => {
        const parsedUrl = new URL(navigationUrl);
        if (parsedUrl.origin !== 'http://localhost:3000' && !navigationUrl.startsWith('file://')) {
            event.preventDefault();
            electron_1.shell.openExternal(navigationUrl);
        }
    });
});
//# sourceMappingURL=main.js.map