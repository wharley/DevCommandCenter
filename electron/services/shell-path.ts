/**
 * Helper to get the user's PATH from a login shell and to run CLIs inside
 * a login shell so that scripts with shebang #!/usr/bin/env node find node
 * when the app is run from a DMG/package (where process.env.PATH is minimal).
 * Includes a filesystem-based fallback (nvm, fnm, Homebrew, /usr/local) when
 * the login shell is unavailable (e.g. app opened from Finder).
 */

import { execSync, spawn, type ChildProcess, type SpawnOptions } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import { platform, homedir } from "node:os";
import { app } from "electron";
import { getNodeBinaryPath } from "./node-binary";

const SHELL_PATH_TIMEOUT_MS = 5000;

/** Escape a string for use inside double-quoted shell argument. */
function escapeDoubleQuoted(s: string): string {
  return s.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
}

/**
 * Builds a shell command string that runs: exec "cliPath" "arg1" "arg2" ...
 * Safe for passing as the single argument to sh -c '...' (no outer quoting needed;
 * spawn passes it as argv).
 */
function buildLoginShellExecCommand(cliPath: string, args: string[]): string {
  const quotedPath = `"${escapeDoubleQuoted(cliPath)}"`;
  const quotedArgs = args.map((a) => `"${escapeDoubleQuoted(a)}"`).join(" ");
  return `exec ${quotedPath} ${quotedArgs}`;
}

/**
 * Returns true when we should run the CLI via a login shell so it inherits
 * the user's PATH (e.g. macOS/Linux when app is launched from DMG/Finder).
 */
export function shouldRunCliViaLoginShell(): boolean {
  return platform() !== "win32";
}

/**
 * Builds a fallback PATH by scanning common locations for the node executable.
 * Used when getLoginShellPath() fails (e.g. app launched from Finder where
 * execSync of login shell may not work). Supports nvm, fnm, Homebrew, and
 * system paths so it works for users with or without nvm.
 */
function getFallbackPathForNode(): string | undefined {
  if (platform() === "win32") return undefined;
  const home = process.env.HOME || homedir();
  const candidates: string[] = [];

  // NVM: ~/.nvm/versions/node/<version>/bin
  const nvmDir = path.join(home, ".nvm", "versions", "node");
  if (fs.existsSync(nvmDir)) {
    try {
      const versions = fs.readdirSync(nvmDir);
      for (const v of versions) {
        const binDir = path.join(nvmDir, v, "bin");
        if (fs.existsSync(path.join(binDir, "node"))) candidates.push(binDir);
      }
    } catch {
      // ignore
    }
  }

  // fnm: ~/.local/share/fnm/current/bin or ~/.fnm/current/bin
  for (const base of [path.join(home, ".local", "share", "fnm", "current"), path.join(home, ".fnm", "current")]) {
    const binDir = path.join(base, "bin");
    if (fs.existsSync(path.join(binDir, "node"))) candidates.push(binDir);
  }

  // Homebrew and system (only add if node exists there)
  for (const dir of ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"]) {
    if (fs.existsSync(path.join(dir, "node"))) candidates.push(dir);
  }

  if (candidates.length === 0) return undefined;
  const basePath = candidates.join(path.delimiter);
  const existing = process.env.PATH;
  return existing ? `${basePath}${path.delimiter}${existing}` : basePath;
}

/**
 * Spawns the CLI so that on macOS/Linux it runs inside a login shell and
 * inherits the user's PATH (fixing "env: node: No such file or directory").
 * On Windows, spawns the CLI directly. Same stdio/options behavior as spawn().
 */
export function spawnCliWithLoginShell(
  cliPath: string,
  args: string[],
  options: SpawnOptions
): ChildProcess {
  if (!shouldRunCliViaLoginShell()) {
    const opts = { ...options };
    if (app.isPackaged) {
      opts.env = { ...opts.env, PATH: getResolvedPathForNode() };
    }
    return spawn(cliPath, args, opts);
  }
  const shell = "/bin/zsh";
  const command = buildLoginShellExecCommand(cliPath, args);
  const resolvedPath = getResolvedPathForNode();
  const env = {
    ...options.env,
    ...(resolvedPath && { PATH: resolvedPath }),
  };
  return spawn(shell, ["-l", "-c", command], {
    ...options,
    cwd: options.cwd,
    env,
    stdio: options.stdio ?? ["ignore", "pipe", "pipe"],
  });
}

/**
 * Returns the PATH to use when invoking node-based CLIs (e.g. from execSync).
 * Use this so scripts with shebang #!/usr/bin/env node find node when the app
 * is launched from Finder/DMG (minimal PATH). When packaged, prepends the
 * bundled Node's directory so the app does not depend on the user's PATH.
 */
export function getResolvedPathForNode(): string {
  const base =
    getLoginShellPath() || getFallbackPathForNode() || process.env.PATH;
  const basePath = base ?? "";
  if (app.isPackaged) {
    const bundledNodeDir = path.dirname(getNodeBinaryPath());
    return basePath
      ? `${bundledNodeDir}${path.delimiter}${basePath}`
      : bundledNodeDir;
  }
  return basePath;
}

/**
 * Returns the PATH from the user's login shell (macOS/Linux only).
 * On Windows or on failure, returns undefined so callers can keep process.env.PATH.
 */
export function getLoginShellPath(): string | undefined {
  if (platform() === "win32") {
    return undefined;
  }

  const shell = "/bin/zsh";
  const home = process.env.HOME || homedir();
  try {
    const path = execSync(`${shell} -l -c 'printf "%s" "$PATH"'`, {
      encoding: "utf8",
      timeout: SHELL_PATH_TIMEOUT_MS,
      env: { ...process.env, HOME: home },
    });
    const trimmed = path?.trim();
    return trimmed || undefined;
  } catch {
    return undefined;
  }
}
