"use client";

import { useEffect } from "react";
import type { Comb, Project } from "@/lib/database/types";

const DEFAULT_TITLE = "Dev Command Center";

function pathBasename(path: string): string {
  const s = path.replace(/[/\\]+$/, "");
  const parts = s.split(/[/\\]/);
  return parts[parts.length - 1] || s;
}

/** Título da janela/aba: branch, pasta do worktree (ou nome do workspace) e projeto. */
export function buildAppWindowTitle(project: Project | null, comb: Comb | null): string {
  if (!project?.name?.trim()) return DEFAULT_TITLE;
  const projectName = project.name.trim();
  if (!comb) return `${projectName} — ${DEFAULT_TITLE}`;
  const branch = (comb.branch ?? comb.baseBranch ?? "").trim() || "—";
  const place = comb.worktreePath?.trim()
    ? pathBasename(comb.worktreePath)
    : (comb.name?.trim() || "Workspace");
  return `${branch} · ${place} · ${projectName} — ${DEFAULT_TITLE}`;
}

/**
 * Atualiza o título da janela nativa (Tauri) e `document.title` (browser / dev).
 * Requer permissão `core:window:allow-set-title` no capability (já coberta pelo default em muitos setups).
 */
export function useAppWindowTitle(project: Project | null, comb: Comb | null) {
  useEffect(() => {
    const title = buildAppWindowTitle(project, comb);
    if (typeof document !== "undefined") {
      document.title = title;
    }
    if (typeof window === "undefined" || !window.desktopAPI) {
      return;
    }
    let cancelled = false;
    void (async () => {
      try {
        const { getCurrentWindow } = await import("@tauri-apps/api/window");
        if (cancelled) return;
        await getCurrentWindow().setTitle(title);
      } catch {
        /* WebView sem Tauri ou capability em falta — document.title já foi aplicado */
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [
    project?.id,
    project?.name,
    comb?.id,
    comb?.name,
    comb?.branch,
    comb?.baseBranch,
    comb?.worktreePath,
  ]);
}
