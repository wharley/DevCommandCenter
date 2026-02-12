/**
 * Stable machine identifier for activation/license.
 * Uses hostname + platform + arch; optional userData path for extra stability.
 */

import crypto from "node:crypto";
import os from "node:os";

let cachedMachineId: string | null = null;

export function getMachineId(userDataPath?: string): string {
  if (cachedMachineId) return cachedMachineId;

  const parts = [
    os.hostname(),
    os.platform(),
    os.arch(),
    process.env.USER ?? process.env.USERNAME ?? "",
    userDataPath ?? "",
  ].filter(Boolean);

  cachedMachineId = crypto
    .createHash("sha256")
    .update(parts.join("|"), "utf8")
    .digest("hex");

  return cachedMachineId;
}
