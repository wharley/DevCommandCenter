/**
 * Permission modes for CLI agents (Codex, Claude Code, Gemini CLI).
 * Maps app permission mode to CLI flags per adapter type.
 * Cursor CLI: no standard flags in dmux; leave empty until CLI exposes them.
 */

import type { PermissionMode } from "../../../lib/database/types";

/** Permission flags per adapter: CLI args for plan / acceptEdits / bypass */
export const adapterPermissionFlags: Record<
  string,
  Partial<Record<NonNullable<PermissionMode>, string>>
> = {
  "claude-code": {
    plan: "--permission-mode plan",
    acceptEdits: "--permission-mode acceptEdits",
    bypass: "--dangerously-skip-permissions",
  },
  codex: {
    acceptEdits: "--full-auto",
    bypass: "--dangerously-bypass-approvals-and-sandbox",
  },
  gemini: {
    plan: "--approval-mode plan",
    acceptEdits: "--approval-mode auto_edit",
    bypass: "--approval-mode yolo",
  },
  // Cursor CLI: permissionFlags vazios no dmux; quando o CLI expuser flags, adicionar aqui.
  cursor: {},
};

/**
 * Resolve CLI permission flags for a provider type and permission mode.
 * Returns array of flag strings (e.g. ["--full-auto"] or ["--permission-mode", "plan"]).
 */
export function getPermissionFlagsForAdapter(
  providerType: string,
  permissionMode: PermissionMode | undefined | null
): string[] {
  const mode = permissionMode || "";
  if (!mode) return [];
  const flags = adapterPermissionFlags[providerType];
  if (!flags) return [];
  const flagStr = flags[mode as keyof typeof flags];
  if (!flagStr) return [];
  return flagStr.trim().split(/\s+/).filter(Boolean);
}
