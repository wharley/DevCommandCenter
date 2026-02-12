import { app } from "electron";
import path from "path";

/**
 * Returns the path to the Node binary to use when running CLI providers.
 * When the app is opened from the OS launcher (e.g. /Applications on macOS,
 * Start Menu on Windows, app launcher on Linux), the process does not
 * inherit the login shell environment (e.g. nvm), so "env node" in CLI shebangs
 * fails with "env: node: No such file or directory". In production we use a
 * bundled Node so the app does not depend on the user's PATH.
 */
export function getNodeBinaryPath(): string {
  if (app.isPackaged) {
    const binaryName = process.platform === "win32" ? "node.exe" : "node";
    return path.join(process.resourcesPath, "bin", binaryName);
  }
  return "node";
}
