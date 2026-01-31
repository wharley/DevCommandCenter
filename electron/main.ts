import { app, BrowserWindow, ipcMain, dialog, shell, nativeImage } from "electron";
import path from "node:path";
import { registerIpcHandlers } from "./ipc-handlers";
import db, { setUserDataPath } from "../lib/database";
import { providerService } from "./services/provider-service";

// Handle creating/removing shortcuts on Windows when installing/uninstalling.
try {
  if (require("electron-squirrel-startup")) {
    app.quit();
  }
} catch {
  // electron-squirrel-startup not available (development mode)
}

let mainWindow: BrowserWindow | null = null;

// Detect development mode
const isDev = process.env.NODE_ENV === "development" || !app.isPackaged;

// Vite dev server port
const VITE_DEV_PORT = 5173;

const ICON_PATH = path.join(__dirname, "..", "..", "public", "icon.png");

function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1400,
    height: 900,
    minWidth: 1000,
    minHeight: 700,
    titleBarStyle: "hiddenInset",
    trafficLightPosition: { x: 16, y: 16 },
    backgroundColor: "#0a0a0f",
    icon: ICON_PATH,
    show: false, // Show when ready to prevent flash
    webPreferences: {
      preload: path.join(__dirname, "preload.js"),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false,
      webSecurity: !isDev, // Relax security in dev for localhost
    },
  });

  // Show window when ready
  mainWindow.once("ready-to-show", () => {
    mainWindow?.show();
  });

  // Load the app
  if (isDev) {
    // Development: load from Vite dev server
    const devUrl = `http://localhost:${VITE_DEV_PORT}`;
    console.log("[Electron] Loading app from Vite dev server:", devUrl);
    mainWindow.loadURL(devUrl);
    mainWindow.webContents.openDevTools();
  } else {
    // Production: load from static build
    const indexPath = path.join(__dirname, "..", "dist", "index.html");
    console.log("[Electron] Loading app from static build:", indexPath);
    mainWindow.loadFile(indexPath);
  }

  mainWindow.on("closed", () => {
    mainWindow = null;
  });

  // Open external links in default browser
  mainWindow.webContents.setWindowOpenHandler(({ url }) => {
    shell.openExternal(url);
    return { action: "deny" };
  });
}

// Initialize database and register IPC handlers
function initializeApp() {
  try {
    // Set the user data path before initializing the database
    const userDataPath = app.getPath("userData");
    console.log("[Electron] User data path:", userDataPath);
    setUserDataPath(userDataPath);

    db.init();
    providerService.migrateLegacyApiKeys();
    console.log("[Electron] Database initialized successfully");
  } catch (error) {
    console.error("[Electron] Failed to initialize database:", error);
    dialog.showErrorBox(
      "Database Error",
      `Failed to initialize the database. The app may not work correctly.\n\nError: ${error}`
    );
  }

  registerIpcHandlers(ipcMain);
}

// App lifecycle
app.whenReady().then(async () => {
  initializeApp();

  // macOS: set dock icon explicitly (BrowserWindow icon doesn't affect dock)
  if (process.platform === "darwin") {
    app.dock.setIcon(nativeImage.createFromPath(ICON_PATH));
  }

  createWindow();

  app.on("activate", () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow();
    }
  });
});

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") {
    app.quit();
  }
});

app.on("before-quit", () => {
  // Close database
  try {
    db.close();
    console.log("[Electron] Database closed successfully");
  } catch (error) {
    console.error("[Electron] Error closing database:", error);
  }
});

// Handle certificate errors in development
if (isDev) {
  app.on(
    "certificate-error",
    (event, _webContents, _url, _error, _certificate, callback) => {
      event.preventDefault();
      callback(true);
    }
  );
}

// Security: Prevent new window creation
app.on("web-contents-created", (_event, contents) => {
  contents.on("will-navigate", (event, navigationUrl) => {
    const parsedUrl = new URL(navigationUrl);
    // Allow navigation within the app (localhost dev or file:// in production)
    if (
      parsedUrl.origin === `http://localhost:${VITE_DEV_PORT}` ||
      navigationUrl.startsWith("file://")
    ) {
      return;
    }
    // Open external URLs in default browser
    event.preventDefault();
    shell.openExternal(navigationUrl);
  });
});
