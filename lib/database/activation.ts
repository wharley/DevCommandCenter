// Dev Command Center - Activation (beta/license) persistence
// Singleton row: one activation record per app installation

import { getDatabase } from "./connection";

export interface ActivationRow {
  email: string;
  machine_id: string;
  activated: number;
  token: string | null;
  activated_at: string;
}

export interface ActivationStatus {
  activated: boolean;
  email?: string;
  activatedAt?: string;
}

export function getActivation(): ActivationStatus | null {
  const db = getDatabase();
  const row = db
    .prepare(
      "SELECT email, machine_id, activated, token, activated_at FROM activation WHERE id = 1",
    )
    .get() as ActivationRow | undefined;

  if (!row) return null;

  return {
    activated: row.activated === 1,
    email: row.email,
    activatedAt: row.activated_at,
  };
}

export function setActivation(params: {
  email: string;
  machineId: string;
  token?: string | null;
}): void {
  const db = getDatabase();
  const now = new Date().toISOString();
  db.prepare(
    `INSERT INTO activation (id, email, machine_id, activated, token, activated_at, created_at, updated_at)
     VALUES (1, @email, @machineId, 1, @token, @now, @now, @now)
     ON CONFLICT(id) DO UPDATE SET
       email = @email,
       machine_id = @machineId,
       activated = 1,
       token = @token,
       activated_at = @now,
       updated_at = @now`,
  ).run({
    email: params.email,
    machineId: params.machineId,
    token: params.token ?? null,
    now,
  });
}
