/**
 * CLI detection per provider type: install test command and common paths.
 * Used to discover CLI path and show "CLI encontrado" / "CLI não encontrado" in settings.
 */

import * as fs from "node:fs";
import * as path from "node:path";
import { execSync, spawnSync } from "node:child_process";
import { platform } from "node:os";

const HOME = process.env.HOME || process.env.USERPROFILE || "";
const homePath = (suffix: string): string[] =>
  HOME ? [path.join(HOME, suffix)] : [];

export type CliProviderType =
  | "claude-code"
  | "codex"
  | "cursor"
  | "gemini";

export interface CliDetectionConfig {
  /** Command name for which/where (e.g. "claude", "codex") */
  command: string;
  /** Shell command to test if CLI is available (e.g. "command -v codex") */
  installTestCommand: string;
  /** Paths to check when which/where fails */
  commonPaths: string[];
  /** Args to run for validation (e.g. ["--version"] or codex: ["-c", "check_for_update_on_startup=false", "--version"]) */
  validateArgs: string[];
}

export const cliDetectionConfig: Record<CliProviderType, CliDetectionConfig> = {
  "claude-code": {
    command: "claude",
    installTestCommand:
      "command -v claude 2>/dev/null || which claude 2>/dev/null",
    commonPaths: [
      ...homePath(".claude/local/claude"),
      ...homePath(".local/bin/claude"),
      "/opt/homebrew/bin/claude",
      "/usr/local/bin/claude",
      "/usr/bin/claude",
      ...homePath("bin/claude"),
    ],
    validateArgs: ["--version"],
  },
  codex: {
    command: "codex",
    installTestCommand:
      "command -v codex 2>/dev/null || which codex 2>/dev/null",
    commonPaths: [
      "/usr/local/bin/codex",
      "/opt/homebrew/bin/codex",
      ...homePath(".local/bin/codex"),
      ...homePath("bin/codex"),
      ...homePath(".npm-global/bin/codex"),
    ],
    validateArgs: ["-c", "check_for_update_on_startup=false", "--version"],
  },
  cursor: {
    command: "cursor-agent",
    installTestCommand:
      "command -v cursor-agent 2>/dev/null || which cursor-agent 2>/dev/null",
    commonPaths: [
      ...homePath(".cursor/bin/cursor-agent"),
      "/usr/local/bin/cursor-agent",
      "/opt/homebrew/bin/cursor-agent",
      ...homePath(".local/bin/cursor-agent"),
      ...homePath("bin/cursor-agent"),
    ],
    validateArgs: ["--version"],
  },
  gemini: {
    command: "gemini",
    installTestCommand:
      "command -v gemini 2>/dev/null || which gemini 2>/dev/null",
    commonPaths: [
      "/usr/local/bin/gemini",
      "/opt/homebrew/bin/gemini",
      ...homePath(".local/bin/gemini"),
      ...homePath("bin/gemini"),
      ...homePath(".npm-global/bin/gemini"),
    ],
    validateArgs: ["--version"],
  },
};

export function getCliDetectionConfig(
  providerType: string
): CliDetectionConfig | null {
  if (providerType in cliDetectionConfig) {
    return cliDetectionConfig[providerType as CliProviderType];
  }
  return null;
}

/**
 * Try to detect CLI path: first run installTestCommand (which), then try commonPaths.
 */
export function detectCliPath(providerType: string): string | null {
  const config = getCliDetectionConfig(providerType);
  if (!config) return null;

  const isWin = platform() === "win32";

  if (isWin) {
    try {
      const out = execSync(`where ${config.command}`, { encoding: "utf8" });
      const firstLine = out.split(/\r?\n/)[0]?.trim();
      if (firstLine) return firstLine;
    } catch {
      // fall through to commonPaths
    }
    for (const p of config.commonPaths) {
      const expanded = p.replace(/^~/, HOME || "");
      if (fs.existsSync(expanded)) return expanded;
    }
    return null;
  }

  const userShell = process.env.SHELL || "zsh";
  try {
    const out = execSync(`${userShell} -l -c 'which ${config.command}'`, {
      encoding: "utf8",
      timeout: 5000,
    });
    const firstLine = out.split(/\r?\n/)[0]?.trim();
    if (firstLine) return firstLine;
  } catch {
    // fall through
  }

  for (const p of config.commonPaths) {
    const expanded = p.replace(/^~/, HOME || "");
    if (fs.existsSync(expanded)) return expanded;
  }
  return null;
}

/**
 * Validate that the given path is a working CLI for this provider type.
 */
export function validateCliPath(
  providerType: string,
  cliPath: string
): { valid: boolean; message?: string } {
  if (!cliPath.trim()) return { valid: false, message: "Path is empty" };
  const config = getCliDetectionConfig(providerType);
  if (!config) return { valid: false, message: "Unknown provider type" };

  const normalized = path.normalize(cliPath.trim());
  if (!fs.existsSync(normalized)) {
    return { valid: false, message: "File not found" };
  }

  try {
    const result = spawnSync(normalized, config.validateArgs, {
      encoding: "utf8",
      timeout: 10000,
      stdio: ["ignore", "pipe", "pipe"],
    });
    if (result.status === 0) return { valid: true };
    const msg = result.stderr || result.error?.message || "Non-zero exit";
    return { valid: false, message: String(msg).slice(0, 200) };
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    return { valid: false, message: msg.slice(0, 200) };
  }
}
