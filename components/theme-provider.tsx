import * as React from "react";
import { createContext, useContext, useEffect, useState } from "react";
import { setTheme as setNativeAppTheme } from "@tauri-apps/api/app";

type Theme = "dark" | "light" | "system";

interface ThemeProviderProps {
  children: React.ReactNode;
  defaultTheme?: Theme;
  storageKey?: string;
  attribute?: string;
  enableSystem?: boolean;
  disableTransitionOnChange?: boolean;
}

interface ThemeProviderState {
  theme: Theme;
  setTheme: (theme: Theme) => void;
  resolvedTheme: "dark" | "light";
}

const ThemeProviderContext = createContext<ThemeProviderState | undefined>(
  undefined,
);

export function ThemeProvider({
  children,
  defaultTheme = "system",
  storageKey = "theme",
  attribute = "class",
  enableSystem = true,
  disableTransitionOnChange = false,
}: ThemeProviderProps) {
  const [theme, setThemeState] = useState<Theme>(() => {
    if (typeof window === "undefined") return defaultTheme;
    return (localStorage.getItem(storageKey) as Theme) || defaultTheme;
  });

  const [resolvedTheme, setResolvedTheme] = useState<"dark" | "light">(() => {
    if (typeof window === "undefined") return "dark";
    if (theme === "system") {
      return window.matchMedia("(prefers-color-scheme: dark)").matches
        ? "dark"
        : "light";
    }
    return theme;
  });

  useEffect(() => {
    const root = window.document.documentElement;

    // Disable transitions if needed
    if (disableTransitionOnChange) {
      root.style.setProperty("transition", "none");
    }

    // Determine the actual theme to apply
    let actualTheme: "dark" | "light";
    if (theme === "system" && enableSystem) {
      actualTheme = window.matchMedia("(prefers-color-scheme: dark)").matches
        ? "dark"
        : "light";
    } else {
      actualTheme = theme === "system" ? "dark" : theme;
    }

    setResolvedTheme(actualTheme);

    // Apply theme based on attribute
    if (attribute === "class") {
      root.classList.remove("light", "dark");
      root.classList.add(actualTheme);
    } else {
      root.setAttribute(attribute, actualTheme);
    }

    // Re-enable transitions
    if (disableTransitionOnChange) {
      // Force reflow
      void root.offsetHeight;
      root.style.removeProperty("transition");
    }
  }, [theme, attribute, enableSystem, disableTransitionOnChange]);

  // Listen for system theme changes
  useEffect(() => {
    if (!enableSystem || theme !== "system") return;

    const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");

    const handleChange = (e: MediaQueryListEvent) => {
      const newTheme = e.matches ? "dark" : "light";
      setResolvedTheme(newTheme);

      const root = window.document.documentElement;
      if (attribute === "class") {
        root.classList.remove("light", "dark");
        root.classList.add(newTheme);
      } else {
        root.setAttribute(attribute, newTheme);
      }
    };

    mediaQuery.addEventListener("change", handleChange);
    return () => mediaQuery.removeEventListener("change", handleChange);
  }, [theme, attribute, enableSystem]);

  // macOS/Windows: alinhar a barra de título nativa ao tema do app (em dev costuma coincidir; no .app empacotado pode divergir).
  useEffect(() => {
    if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) return;
    if (enableSystem && theme === "system") {
      void setNativeAppTheme(null);
    } else {
      void setNativeAppTheme(resolvedTheme);
    }
  }, [theme, resolvedTheme, enableSystem]);

  const setTheme = (newTheme: Theme) => {
    localStorage.setItem(storageKey, newTheme);
    setThemeState(newTheme);
  };

  const value: ThemeProviderState = {
    theme,
    setTheme,
    resolvedTheme,
  };

  return (
    <ThemeProviderContext.Provider value={value}>
      {children}
    </ThemeProviderContext.Provider>
  );
}

export function useTheme() {
  const context = useContext(ThemeProviderContext);

  if (context === undefined) {
    throw new Error("useTheme must be used within a ThemeProvider");
  }

  return context;
}
