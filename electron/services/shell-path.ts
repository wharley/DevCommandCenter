/**
 * Helper to get the user's PATH from a login shell and to run CLIs inside
 * a login shell so that scripts with shebang #!/usr/bin/env node find node
 * when the app is run from a DMG/package (where process.env.PATH is minimal).
 */

import { execSync, spawn, type ChildProcess, type SpawnOptions } from "node:child_process";
import { platform } from "node:os";

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
    return spawn(cliPath, args, options);
  }
  const shell = "/bin/zsh";
  const command = buildLoginShellExecCommand(cliPath, args);
  return spawn(shell, ["-l", "-c", command], {
    ...options,
    cwd: options.cwd,
    env: options.env,
    stdio: options.stdio ?? ["ignore", "pipe", "pipe"],
  });
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
  try {
    const path = execSync(`${shell} -l -c 'printf "%s" "$PATH"'`, {
      encoding: "utf8",
      timeout: SHELL_PATH_TIMEOUT_MS,
      env: { ...process.env, HOME: process.env.HOME },
    });
    const trimmed = path?.trim();
    return trimmed || undefined;
  } catch {
    return undefined;
  }
}
