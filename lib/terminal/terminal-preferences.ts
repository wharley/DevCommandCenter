/** Preferências do terminal embutido (xterm) — persistidas em localStorage. */

export const TERMINAL_PREFS_STORAGE_KEY = "dcc:terminal:appearance";

export interface TerminalAppearancePreferences {
  /** Tamanho base da fonte (px lógicos no xterm). */
  fontSize: number;
  /** Família CSS (ex.: var(--font-geist-mono) ou Menlo). */
  fontFamily: string;
  /** Quando true, cores seguem o tema claro/escuro do app. */
  useAppThemeColors: boolean;
  /** Linhas de histórico do scrollback (100-50000). */
  scrollback: number;
  /** Estilo do cursor. */
  cursorStyle: "block" | "underline" | "bar";
  /** Cursor piscante. */
  cursorBlink: boolean;
  /** Copiar automaticamente ao selecionar texto. */
  copyOnSelect: boolean;
  /** Clique direito seleciona palavra. */
  rightClickSelectsWord: boolean;
  /** Estilo do bell (nenhum, visual, som, ambos). */
  bellStyle: "none" | "visual" | "sound" | "both";
  /** Sensibilidade de scroll rápido (Alt+Scroll). */
  fastScrollSensitivity: number;
}

const DEFAULT_FONT_SIZE = 13;

const DEFAULTS: TerminalAppearancePreferences = {
  fontSize: DEFAULT_FONT_SIZE,
  fontFamily: "var(--font-geist-mono, 'Menlo', 'Monaco', monospace)",
  useAppThemeColors: true,
  scrollback: 10000,
  cursorStyle: "block",
  cursorBlink: true,
  copyOnSelect: false,
  rightClickSelectsWord: true,
  bellStyle: "visual",
  fastScrollSensitivity: 5,
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
        typeof parsed.useAppThemeColors === "boolean"
          ? parsed.useAppThemeColors
          : DEFAULTS.useAppThemeColors,
      scrollback:
        typeof parsed.scrollback === "number" &&
        parsed.scrollback >= 100 &&
        parsed.scrollback <= 50000
          ? parsed.scrollback
          : DEFAULTS.scrollback,
      cursorStyle:
        parsed.cursorStyle === "block" ||
        parsed.cursorStyle === "underline" ||
        parsed.cursorStyle === "bar"
          ? parsed.cursorStyle
          : DEFAULTS.cursorStyle,
      cursorBlink:
        typeof parsed.cursorBlink === "boolean"
          ? parsed.cursorBlink
          : DEFAULTS.cursorBlink,
      copyOnSelect:
        typeof parsed.copyOnSelect === "boolean"
          ? parsed.copyOnSelect
          : DEFAULTS.copyOnSelect,
      rightClickSelectsWord:
        typeof parsed.rightClickSelectsWord === "boolean"
          ? parsed.rightClickSelectsWord
          : DEFAULTS.rightClickSelectsWord,
      bellStyle:
        parsed.bellStyle === "none" ||
        parsed.bellStyle === "visual" ||
        parsed.bellStyle === "sound" ||
        parsed.bellStyle === "both"
          ? parsed.bellStyle
          : DEFAULTS.bellStyle,
      fastScrollSensitivity:
        typeof parsed.fastScrollSensitivity === "number" &&
        parsed.fastScrollSensitivity >= 1 &&
        parsed.fastScrollSensitivity <= 20
          ? parsed.fastScrollSensitivity
          : DEFAULTS.fastScrollSensitivity,
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

/** Alias for loadTerminalAppearance */
export const getTerminalAppearancePreferences = loadTerminalAppearance;
