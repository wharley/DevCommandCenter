// Dev Command Center - Normalize IPC data
// Converte campos de data (strings ISO após IPC) para Date no renderer

const DATE_KEYS: Record<string, string[]> = {
  project: ['lastOpenedAt', 'createdAt', 'updatedAt'],
  provider: ['createdAt', 'updatedAt'],
  comb: ['lastOpenedAt', 'lastGitActivityAt', 'createdAt', 'updatedAt'],
  pane: ['lastActivityAt', 'createdAt', 'updatedAt'],
};

function parseDate(value: unknown): Date | null {
  if (value instanceof Date) return value;
  if (typeof value === 'string') {
    const raw = value.trim();
    const d = new Date(raw);
    if (!isNaN(d.getTime())) return d;

    // SQLite/Tauri commonly returns "YYYY-MM-DD HH:mm:ss" (no timezone).
    const sqliteMatch = raw.match(
      /^(\d{4})-(\d{2})-(\d{2})[ T](\d{2}):(\d{2}):(\d{2})$/,
    );
    if (sqliteMatch) {
      const [, y, m, day, hh, mm, ss] = sqliteMatch;
      const localDate = new Date(
        Number(y),
        Number(m) - 1,
        Number(day),
        Number(hh),
        Number(mm),
        Number(ss),
      );
      return isNaN(localDate.getTime()) ? null : localDate;
    }

    return null;
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
  const normalized = normalizeDates(raw, DATE_KEYS.project);
  const repoConfig = normalized.repoConfig ?? normalized.repo_config;
  if (typeof repoConfig === "string" && repoConfig.trim()) {
    try {
      normalized.repoConfig = JSON.parse(repoConfig) as unknown;
    } catch {
      /* ignore */
    }
  }
  return normalized;
}

export function normalizeProvider(raw: Record<string, unknown>): Record<string, unknown> {
  return normalizeDates(raw, DATE_KEYS.provider);
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

export function normalizeComb(raw: Record<string, unknown>): Record<string, unknown> {
  const normalized = normalizeDates(raw, DATE_KEYS.comb);
  const rt = normalized.review_targets ?? normalized.reviewTargets;
  if (typeof rt === "string" && rt.trim()) {
    try {
      normalized.reviewTargets = JSON.parse(rt) as unknown;
    } catch {
      /* ignore */
    }
  }
  const fl = normalized.forge_link ?? normalized.forgeLink;
  if (typeof fl === "string" && fl.trim()) {
    try {
      normalized.forgeLink = JSON.parse(fl) as unknown;
    } catch {
      /* ignore */
    }
  }
  return normalized;
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
