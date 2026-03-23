"use client";

import { useCallback, useMemo } from "react";

/**
 * Shell desktop (Tauri): APIs nativas e `window.db` expostos pelo bridge.
 */
export function useDesktopShell() {
  const isDesktop = useMemo(() => {
    if (typeof window === "undefined") return false;
    return !!window.desktopAPI;
  }, []);

  const api = useMemo(() => {
    if (typeof window === "undefined") return null;
    return window.desktopAPI ?? null;
  }, []);

  const db = useMemo(() => {
    if (typeof window === "undefined") return null;
    return window.db ?? null;
  }, []);

  const platform = useMemo(() => {
    return api?.platform ?? "web";
  }, [api]);

  const selectDirectory = useCallback(async () => {
    if (!api) return null;
    return api.dialog.selectDirectory();
  }, [api]);

  const confirm = useCallback(async (message: string) => {
    if (!api) {
      return window.confirm(message);
    }
    return api.dialog.confirm(message);
  }, [api]);

  const openExternal = useCallback(async (url: string) => {
    if (!api) {
      window.open(url, "_blank");
      return;
    }
    await api.shell.openExternal(url);
  }, [api]);

  const openPath = useCallback(async (path: string) => {
    if (!api) {
      console.warn("openPath is only available in the desktop shell");
      return;
    }
    await api.shell.openPath(path);
  }, [api]);

  const showItemInFolder = useCallback(async (path: string) => {
    if (!api) {
      console.warn("showItemInFolder is only available in the desktop shell");
      return;
    }
    await api.shell.showItemInFolder(path);
  }, [api]);

  const resolveCliPath = useCallback(
    async (command: string): Promise<string | null> => {
      if (!api?.shell?.resolveCliPath) return null;
      const result = await api.shell.resolveCliPath(command);
      return result?.path ?? null;
    },
    [api]
  );

  const detectCliForProvider = useCallback(
    async (providerType: string): Promise<string | null> => {
      if (!api?.shell?.detectCliForProvider) return null;
      const result = await api.shell.detectCliForProvider(providerType);
      return result?.path ?? null;
    },
    [api]
  );

  const validateCliPath = useCallback(
    async (
      providerType: string,
      cliPath: string
    ): Promise<{ valid: boolean; message?: string }> => {
      if (!api?.shell?.validateCliPath)
        return { valid: false, message: "Not available" };
      return api.shell.validateCliPath(providerType, cliPath);
    },
    [api]
  );

  return {
    isDesktop,
    platform,
    api,
    db,
    selectDirectory,
    confirm,
    openExternal,
    openPath,
    showItemInFolder,
    resolveCliPath,
    detectCliForProvider,
    validateCliPath,
  };
}

/** True quando o bridge instalou `window.desktopAPI` (ex.: app Tauri). */
export function useIsDesktopShell() {
  return useMemo(() => {
    if (typeof window === "undefined") return false;
    return !!window.desktopAPI;
  }, []);
}
