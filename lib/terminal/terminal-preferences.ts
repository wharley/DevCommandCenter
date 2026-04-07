/** Preferências do terminal embutido (xterm) — persistidas em localStorage. */

export const TERMINAL_PREFS_STORAGE_KEY = "dcc:terminal:appearance";

export interface TerminalAppearancePreferences {
  /** Tamanho base da fonte (px lógicos no xterm). */
  fontSize: number;
  /** Família CSS (ex.: var(--font-geist-mono) ou Menlo). */
  fontFamily: string;
  /** Quando true, cores seguem o tema claro/escuro do app. */
  useAppThemeColors: boolean;
}

const DEFAULT_FONT_SIZE = 13;

const DEFAULTS: TerminalAppearancePreferences = {
  fontSize: DEFAULT_FONT_SIZE,
  fontFamily: "var(--font-geist-mono, 'Menlo', 'Monaco', monospace)",
  useAppThemeColors: true,
};

export function loadTerminalAppearance(): TerminalAppearancePreferences {
  if (typeof window === "undefined") return { ...DEFAULTS };
  try {
    const raw = localStorage.getItem(TERMINAL_PREFS_STORAGE_KEY);
    if (!raw) return { ...DEFAULTS };
    const parsed = JSON.parse(raw) as Partial<TerminalAppearancePreferences>;
    return {
      fontSize:
        typeof parsed.fontSize === "number" &&
        parsed.fontSize >= 8 &&
        parsed.fontSize <= 32
          ? parsed.fontSize
          : DEFAULTS.fontSize,
      fontFamily:
        typeof parsed.fontFamily === "string" && parsed.fontFamily.trim().length > 0
          ? parsed.fontFamily.trim()
          : DEFAULTS.fontFamily,
      useAppThemeColors:
        typeof parsed.useAppThemeColors === "boolean" ? parsed.useAppThemeColors : DEFAULTS.useAppThemeColors,
    };
  } catch {
    return { ...DEFAULTS };
  }
}

export function saveTerminalAppearance(prefs: TerminalAppearancePreferences): void {
  if (typeof window === "undefined") return;
  localStorage.setItem(TERMINAL_PREFS_STORAGE_KEY, JSON.stringify(prefs));
  notifyTerminalPrefsChanged();
}

export function notifyTerminalPrefsChanged(): void {
  if (typeof window === "undefined") return;
  window.dispatchEvent(new CustomEvent("dcc-terminal-prefs-changed"));
}

export function bumpTerminalFontSize(delta: number): TerminalAppearancePreferences {
  const cur = loadTerminalAppearance();
  const nextSize = Math.min(32, Math.max(8, Math.round(cur.fontSize + delta)));
  const next = { ...cur, fontSize: nextSize };
  saveTerminalAppearance(next);
  return next;
}

export function resetTerminalFontSize(): TerminalAppearancePreferences {
  const cur = loadTerminalAppearance();
  const next = { ...cur, fontSize: DEFAULT_FONT_SIZE };
  saveTerminalAppearance(next);
  return next;
}

export const terminalPrefsDefaults = DEFAULTS;
