import { useCallback, useEffect, useState } from "react";

export interface TerminalProjectActivity {
  totalRunningPanes: number;
  runningPanesByCombId: Record<string, number>;
}

const emptyActivity: TerminalProjectActivity = {
  totalRunningPanes: 0,
  runningPanesByCombId: {},
};

/**
 * Agrega PTYs com status "running" por Missão (comb) no projeto atual.
 * Usado na sidebar do Hive para ícones e contador.
 */
export function useTerminalProjectActivity(projectId: string | null) {
  const [activity, setActivity] =
    useState<TerminalProjectActivity>(emptyActivity);

  const refresh = useCallback(async () => {
    if (!projectId || !window.electronAPI?.terminal?.getProjectActivity) {
      setActivity(emptyActivity);
      return;
    }
    try {
      const next = await window.electronAPI.terminal.getProjectActivity(
        projectId,
      );
      setActivity(next);
    } catch {
      setActivity(emptyActivity);
    }
  }, [projectId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    if (!projectId || !window.electronAPI?.terminal?.onExit) return;
    const off = window.electronAPI.terminal.onExit(() => {
      void refresh();
    });
    return off;
  }, [projectId, refresh]);

  useEffect(() => {
    if (!projectId) return;
    const t = window.setInterval(() => {
      void refresh();
    }, 1500);
    return () => window.clearInterval(t);
  }, [projectId, refresh]);

  return { activity, refresh };
}
