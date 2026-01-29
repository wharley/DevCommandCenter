'use client';

import { useCallback, useMemo } from 'react';

/**
 * Hook to detect if running in Electron environment
 * and provide access to Electron APIs
 */
export function useElectron() {
  const isElectron = useMemo(() => {
    if (typeof window === 'undefined') return false;
    return !!window.electronAPI;
  }, []);

  const api = useMemo(() => {
    if (typeof window === 'undefined') return null;
    return window.electronAPI ?? null;
  }, []);

  const db = useMemo(() => {
    if (typeof window === 'undefined') return null;
    return window.db ?? null;
  }, []);

  const platform = useMemo(() => {
    return api?.platform ?? 'web';
  }, [api]);

  // Dialog helpers
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

  // Shell helpers
  const openExternal = useCallback(async (url: string) => {
    if (!api) {
      window.open(url, '_blank');
      return;
    }
    await api.shell.openExternal(url);
  }, [api]);

  const openPath = useCallback(async (path: string) => {
    if (!api) {
      console.warn('openPath is only available in Electron');
      return;
    }
    await api.shell.openPath(path);
  }, [api]);

  const showItemInFolder = useCallback(async (path: string) => {
    if (!api) {
      console.warn('showItemInFolder is only available in Electron');
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

  return {
    isElectron,
    platform,
    api,
    db,
    // Dialog
    selectDirectory,
    confirm,
    // Shell
    openExternal,
    openPath,
    showItemInFolder,
    resolveCliPath,
  };
}

/**
 * Hook to check if we're running in Electron
 */
export function useIsElectron() {
  return useMemo(() => {
    if (typeof window === 'undefined') return false;
    return !!window.electronAPI;
  }, []);
}
