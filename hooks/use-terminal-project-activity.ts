import { useCallback, useEffect, useState } from "react";
import type { TerminalProjectActivity } from "@/types/app";

const emptyActivity: TerminalProjectActivity = {
  totalRunningPanes: 0,
  runningPanesByCombId: {},
  activeAgentsByCombId: {},
  workingAgents: 0,
  waitingAgents: 0,
  activeAgents: [],
};

/**
 * Agrega PTYs com status "running" por Missão (comb) no projeto atual.
 * Usado na sidebar do Hive para ícones e contador.
 */
export function useTerminalProjectActivity(projectId: string | null) {
  const [activity, setActivity] =
    useState<TerminalProjectActivity>(emptyActivity);

  const refresh = useCallback(async () => {
    if (!projectId || !window.desktopAPI?.terminal?.getProjectActivity) {
      setActivity(emptyActivity);
      return;
    }
    try {
      const next = await window.desktopAPI.terminal.getProjectActivity(
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
    if (!projectId || !window.desktopAPI?.terminal?.onExit) return;
    const off = window.desktopAPI.terminal.onExit(() => {
      void refresh();
    });
    return off;
  }, [projectId, refresh]);

  useEffect(() => {
    if (!projectId || !window.desktopAPI?.terminal?.onActivity) return;
    const off = window.desktopAPI.terminal.onActivity((payload) => {
      if (payload.projectId && payload.projectId !== projectId) return;
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
