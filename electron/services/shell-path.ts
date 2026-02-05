/**
 * Helper to get the user's PATH from a login shell.
 * Used when spawning provider CLIs (e.g. Claude Code, Codex, Cursor) so that
 * scripts with shebang #!/usr/bin/env node can find node when the app is
 * run from a DMG/package (where process.env.PATH is minimal).
 */

import { execSync } from "node:child_process";
import { platform } from "node:os";

const SHELL_PATH_TIMEOUT_MS = 5000;

/**
 * Returns the PATH from the user's login shell (macOS/Linux only).
 * On Windows or on failure, returns undefined so callers can keep process.env.PATH.
 */
export function getLoginShellPath(): string | undefined {
  if (platform() === "win32") {
    return undefined;
  }

  const shell = process.env.SHELL || "zsh";
  try {
    const path = execSync(`${shell} -l -c 'printf "%s" "$PATH"'`, {
      encoding: "utf8",
      timeout: SHELL_PATH_TIMEOUT_MS,
    });
    const trimmed = path?.trim();
    return trimmed || undefined;
  } catch {
    return undefined;
  }
}
