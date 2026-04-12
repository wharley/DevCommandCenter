import { useEffect, useRef } from "react";
import { toast } from "sonner";
import {
  areNotificationsEnabled,
  showNativeNotification,
  type NativeNotificationAction,
} from "@/lib/notifications";
import {
  type AgentTerminalPhase,
  type TerminalAttentionPayload,
  resolveAttentionPhase,
} from "@/lib/terminal/attention-types";
import type { NativeNotificationActionEvent } from "@/types/app";

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
  /** Estado estruturado (Maestro-style). Registos antigos podem omitir. */
  phase?: AgentTerminalPhase;
  excerpt: string | null;
  createdAt: number;
  read: boolean;
}

const RENDERER_DEDUPE_MS = 95_000;
const ACTIONS_WITH_NAV: NativeNotificationAction[] = [
  { id: "reply", label: "Abrir painel" },
  { id: "dismiss", label: "Dispensar" },
];
const ACTIONS_DISMISS_ONLY: NativeNotificationAction[] = [
  { id: "dismiss", label: "Dispensar" },
];

/**
 * Subscreve `terminal:attention` (main), resolve nomes (projeto · missão · excerto),
 * mostra toast Sonner e opcionalmente navega para o painel.
 */
export function useTerminalAttentionToasts(options?: {
  onNavigateToPane?: (detail: NavigateToPaneDetail) => void;
  onAttentionRecord?: (record: TerminalAttentionRecord) => void;
  onAttentionAction?: (event: NativeNotificationActionEvent) => void;
  /**
   * Quando devolve true, o utilizador está a ver este pane no workspace (sem painel Providers por cima).
   * Nesse caso evitamos notificação nativa duplicada; o toast in-app mantém-se.
   */
  isAttentionPaneInView?: (detail: {
    paneId: string;
    combId: string;
  }) => boolean;
}) {
  const navigateRef = useRef(options?.onNavigateToPane);
  navigateRef.current = options?.onNavigateToPane;
  const recordRef = useRef(options?.onAttentionRecord);
  recordRef.current = options?.onAttentionRecord;
  const actionRef = useRef(options?.onAttentionAction);
  actionRef.current = options?.onAttentionAction;
  const inViewRef = useRef(options?.isAttentionPaneInView);
  inViewRef.current = options?.isAttentionPaneInView;
  const lastRendererEmit = useRef<Map<string, number>>(new Map());

  useEffect(() => {
    const subscribe = window.desktopAPI?.terminal?.onAttention;
    if (!subscribe) return;

    return subscribe(async (payload: TerminalAttentionPayload) => {
      const attentionId = payload.paneId ?? payload.ptyId ?? "unknown";
      const phase = resolveAttentionPhase(payload);
      const reason = payload.reason ?? payload.status ?? phase;
      const dedupeKey = `${attentionId}:${phase}:${payload.excerpt?.slice(0, 64) ?? ""}`;
      const notificationId =
        payload.notificationId ??
        `${attentionId}:${phase}:${payload.excerpt?.slice(0, 64) ?? ""}`;
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
      const projectId = comb?.projectId ?? project?.id ?? "";

      const projectName = project?.name ?? "Projeto";
      const missionName = comb?.name ?? "Missão";

      const reasonLine =
        phase === "idle"
          ? "Sem saída há um tempo - pode estar a espera de interação no terminal."
          : phase === "error"
            ? "Possivel erro ou falha reportada no terminal."
            : "O agente pode precisar da tua atenção no terminal.";

      const excerpt =
        payload.excerpt && payload.excerpt.trim().length > 0
          ? payload.excerpt.trim()
          : null;

      const description = excerpt ? `${reasonLine}\n${excerpt}` : reasonLine;
      const title = `${projectName} · ${missionName}`;

      const nav =
        comb && project
          ? {
              projectId,
              combId: pane.combId,
              paneId: pane.id,
            }
          : null;
      const record: TerminalAttentionRecord | null =
        nav && payload.paneId && comb && project
          ? {
              id: notificationId,
              paneId: payload.paneId,
              combId: pane.combId,
              projectId,
              projectName,
              workspaceName: missionName,
              reason: typeof reason === "string" ? reason : phase,
              phase,
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

      const viewingThisPane =
        inViewRef.current?.({
          paneId: pane.id,
          combId: pane.combId,
        }) ?? false;
      const suppressOsBanner =
        viewingThisPane &&
        typeof document !== "undefined" &&
        document.visibilityState === "visible" &&
        document.hasFocus();

      if (areNotificationsEnabled() && !suppressOsBanner) {
        void showNativeNotification({
          title,
          body: description,
          icon: "auto",
          notificationId,
          source: "terminal-attention",
          paneId: payload.paneId,
          combId: pane.combId,
          projectId,
          actions: nav ? ACTIONS_WITH_NAV : ACTIONS_DISMISS_ONLY,
        });
      }
    });
  }, []);

  useEffect(() => {
    const subscribe = window.desktopAPI?.app?.onNotificationAction;
    if (!subscribe) return;

    return subscribe((payload: NativeNotificationActionEvent) => {
      if (payload.source && payload.source !== "terminal-attention") return;

      actionRef.current?.(payload);

      if (payload.actionId === "dismiss" || payload.actionId === "__closed") {
        return;
      }

      if (
        payload.projectId &&
        payload.combId &&
        payload.paneId &&
        navigateRef.current
      ) {
        navigateRef.current({
          projectId: payload.projectId,
          combId: payload.combId,
          paneId: payload.paneId,
        });
      }
    });
  }, []);
}
