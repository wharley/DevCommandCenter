// Dev Command Center - Normalize IPC data
// Converte campos de data (strings ISO após IPC) para Date no renderer

const DATE_KEYS: Record<string, string[]> = {
  project: ['lastOpenedAt', 'createdAt', 'updatedAt'],
  provider: ['createdAt', 'updatedAt'],
  mission: ['startedAt', 'completedAt', 'createdAt', 'updatedAt'],
  missionLog: ['createdAt'],
  comb: ['lastOpenedAt', 'createdAt', 'updatedAt'],
  pane: ['lastActivityAt', 'createdAt', 'updatedAt'],
};

function parseDate(value: unknown): Date | null {
  if (value instanceof Date) return value;
  if (typeof value === 'string') {
    const d = new Date(value);
    return isNaN(d.getTime()) ? null : d;
  }
  return null;
}

function normalizeDates<T extends Record<string, unknown>>(
  obj: T,
  dateKeys: string[],
): T {
  const out = { ...obj };
  for (const key of dateKeys) {
    if (key in out && out[key] != null) {
      const parsed = parseDate(out[key]);
      if (parsed) (out as Record<string, unknown>)[key] = parsed;
    }
  }
  return out;
}

export function normalizeProject(raw: Record<string, unknown>): Record<string, unknown> {
  return normalizeDates(raw, DATE_KEYS.project);
}

export function normalizeProvider(raw: Record<string, unknown>): Record<string, unknown> {
  return normalizeDates(raw, DATE_KEYS.provider);
}

export function normalizeMission(raw: Record<string, unknown>): Record<string, unknown> {
  const normalized = normalizeDates(raw, DATE_KEYS.mission);
  
  // Normalize dates inside pendingCommands array
  if (Array.isArray(normalized.pendingCommands)) {
    normalized.pendingCommands = normalized.pendingCommands.map((cmd: Record<string, unknown>) => {
      if (cmd && typeof cmd === 'object' && cmd.confirmedAt) {
        return { ...cmd, confirmedAt: parseDate(cmd.confirmedAt) };
      }
      return cmd;
    });
  }
  
  return normalized;
}

export function normalizeMissionLog(raw: Record<string, unknown>): Record<string, unknown> {
  return normalizeDates(raw, DATE_KEYS.missionLog);
}

export function normalizeProjects(raw: unknown[]): unknown[] {
  return raw.map((item) =>
    typeof item === 'object' && item !== null
      ? normalizeProject(item as Record<string, unknown>)
      : item,
  );
}

export function normalizeProviders(raw: unknown[]): unknown[] {
  return raw.map((item) =>
    typeof item === 'object' && item !== null
      ? normalizeProvider(item as Record<string, unknown>)
      : item,
  );
}

export function normalizeMissions(raw: unknown[]): unknown[] {
  return raw.map((item) =>
    typeof item === 'object' && item !== null
      ? normalizeMission(item as Record<string, unknown>)
      : item,
  );
}

export function normalizeMissionLogs(raw: unknown[]): unknown[] {
  return raw.map((item) =>
    typeof item === 'object' && item !== null
      ? normalizeMissionLog(item as Record<string, unknown>)
      : item,
  );
}

export function normalizeComb(raw: Record<string, unknown>): Record<string, unknown> {
  return normalizeDates(raw, DATE_KEYS.comb);
}

export function normalizePane(raw: Record<string, unknown>): Record<string, unknown> {
  return normalizeDates(raw, DATE_KEYS.pane);
}

export function normalizeCombs(raw: unknown[]): unknown[] {
  return raw.map((item) =>
    typeof item === 'object' && item !== null
      ? normalizeComb(item as Record<string, unknown>)
      : item,
  );
}

export function normalizePanes(raw: unknown[]): unknown[] {
  return raw.map((item) =>
    typeof item === 'object' && item !== null
      ? normalizePane(item as Record<string, unknown>)
      : item,
  );
}
