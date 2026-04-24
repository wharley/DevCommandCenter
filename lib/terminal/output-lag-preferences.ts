/** Preferências de sensibilidade para a deteção de “ligação lenta”. */

export const TERMINAL_OUTPUT_LAG_PREFS_STORAGE_KEY = "dcc:terminal:output-lag";

export interface TerminalOutputLagPreferences {
  /**
   * Multiplicador de sensibilidade.
   * Menor = deteta mais cedo. Maior = tolera mais backlog/latência.
   */
  sensitivity: number;
}

const DEFAULTS: TerminalOutputLagPreferences = {
  sensitivity: 1,
};

function clampSensitivity(value: number): number {
  if (!Number.isFinite(value)) return DEFAULTS.sensitivity;
  return Math.min(1.5, Math.max(0.75, Math.round(value * 100) / 100));
}

export function loadTerminalOutputLagPreferences(): TerminalOutputLagPreferences {
  if (typeof window === "undefined") return { ...DEFAULTS };
  try {
    const raw = localStorage.getItem(TERMINAL_OUTPUT_LAG_PREFS_STORAGE_KEY);
    if (!raw) return { ...DEFAULTS };
    const parsed = JSON.parse(raw) as Partial<TerminalOutputLagPreferences>;
    return {
      sensitivity: clampSensitivity(
        typeof parsed.sensitivity === "number" ? parsed.sensitivity : DEFAULTS.sensitivity,
      ),
    };
  } catch {
    return { ...DEFAULTS };
  }
}

export function saveTerminalOutputLagPreferences(
  prefs: TerminalOutputLagPreferences,
): void {
  if (typeof window === "undefined") return;
  localStorage.setItem(
    TERMINAL_OUTPUT_LAG_PREFS_STORAGE_KEY,
    JSON.stringify({
      sensitivity: clampSensitivity(prefs.sensitivity),
    }),
  );
  notifyTerminalOutputLagPrefsChanged();
}

export function notifyTerminalOutputLagPrefsChanged(): void {
  if (typeof window === "undefined") return;
  window.dispatchEvent(new CustomEvent("dcc-terminal-output-lag-prefs-changed"));
}

export const terminalOutputLagPreferencesDefaults = DEFAULTS;
