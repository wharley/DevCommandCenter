import { useEffect, useRef } from "react";
import { toast } from "sonner";
import type { TerminalAttentionPayload } from "@/lib/terminal/attention-types";

export interface NavigateToPaneDetail {
  projectId: string;
  combId: string;
  paneId: string;
}

export interface TerminalAttentionRecord {
  id: string;
  paneId: string;
  combId: string;
  projectId: string;
  projectName: string;
  workspaceName: string;
  reason: string;
  excerpt: string | null;
  createdAt: number;
  read: boolean;
}

const RENDERER_DEDUPE_MS = 95_000;

/**
 * Subscreve `terminal:attention` (main), resolve nomes (projeto · missão · excerto),
 * mostra toast Sonner e opcionalmente navega para o painel.
 */
export function useTerminalAttentionToasts(options?: {
  onNavigateToPane?: (detail: NavigateToPaneDetail) => void;
  onAttentionRecord?: (record: TerminalAttentionRecord) => void;
}) {
  const navigateRef = useRef(options?.onNavigateToPane);
  navigateRef.current = options?.onNavigateToPane;
  const recordRef = useRef(options?.onAttentionRecord);
  recordRef.current = options?.onAttentionRecord;
  const lastRendererEmit = useRef<Map<string, number>>(new Map());

  useEffect(() => {
    const subscribe = window.desktopAPI?.terminal?.onAttention;
    if (!subscribe) return;

    return subscribe(async (payload: TerminalAttentionPayload) => {
      const attentionId = payload.paneId ?? payload.ptyId ?? "unknown";
      const reason = payload.reason ?? payload.status ?? "waiting";
      const dedupeKey = `${attentionId}:${reason}:${payload.excerpt?.slice(0, 64) ?? ""}`;
      const now = Date.now();
      const prev = lastRendererEmit.current.get(dedupeKey) ?? 0;
      if (now - prev < RENDERER_DEDUPE_MS) return;
      lastRendererEmit.current.set(dedupeKey, now);
      if (lastRendererEmit.current.size > 120) {
        for (const [k, t] of lastRendererEmit.current) {
          if (now - t > 3_600_000) lastRendererEmit.current.delete(k);
        }
      }

      if (!window.db?.panes?.findById) return;
      if (!payload.paneId) return;
      const pane = await window.db.panes.findById(payload.paneId);
      if (!pane) return;
      const comb = window.db.combs?.findById
        ? await window.db.combs.findById(pane.combId)
        : null;
      const project =
        comb && window.db.projects?.findById
          ? await window.db.projects.findById(comb.projectId)
          : null;

      const projectName = project?.name ?? "Projeto";
      const missionName = comb?.name ?? "Missão";

      const reasonLine =
        payload.reason === "idle"
          ? "Sem saída há um tempo — pode estar à espera de interação no terminal."
          : "O agente pode precisar da tua atenção no terminal.";

      const excerpt =
        payload.excerpt && payload.excerpt.trim().length > 0
          ? payload.excerpt.trim()
          : null;

      const description = excerpt
        ? `${reasonLine}\n${excerpt}`
        : reasonLine;

      const title = `${projectName} · ${missionName}`;

      const nav =
        comb && project
          ? {
              projectId: comb.projectId,
              combId: pane.combId,
              paneId: pane.id,
            }
          : null;
      const record: TerminalAttentionRecord | null =
        nav && payload.paneId
          ? {
              id: `${payload.paneId}:${now}`,
              paneId: payload.paneId,
              combId: pane.combId,
              projectId: comb.projectId,
              projectName,
              workspaceName: missionName,
              reason: typeof reason === "string" ? reason : "waiting",
              excerpt,
              createdAt: now,
              read: false,
            }
          : null;
      if (record) {
        recordRef.current?.(record);
      }

      toast.info(title, {
        description,
        duration: 22_000,
        ...(nav && navigateRef.current
          ? {
              action: {
                label: "Ir ao painel",
                onClick: () => navigateRef.current?.(nav),
              },
            }
          : {}),
      });
    });
  }, []);
}
