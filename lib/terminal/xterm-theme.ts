import type { ITheme } from "xterm";

/** Paletas alinhadas ao app (dark default) e modo claro. */
export function getXtermColorTheme(resolved: "dark" | "light", useAppColors: boolean): ITheme {
  if (!useAppColors) {
    return darkPalette;
  }
  return resolved === "light" ? lightPalette : darkPalette;
}

const darkPalette: ITheme = {
  background: "#1a1a1a",
  foreground: "#e5e5e5",
  cursor: "#e5e5e5",
  black: "#1e1e1e",
  red: "#cd3131",
  green: "#0dbc79",
  yellow: "#e5e510",
  blue: "#2472c8",
  magenta: "#bc3fbc",
  cyan: "#11a8cd",
  white: "#e5e5e5",
};

const lightPalette: ITheme = {
  background: "#fafafa",
  foreground: "#171717",
  cursor: "#171717",
  black: "#0a0a0a",
  red: "#b91c1c",
  green: "#15803d",
  yellow: "#a16207",
  blue: "#1d4ed8",
  magenta: "#a21caf",
  cyan: "#0e7490",
  white: "#fafafa",
};
