/**
 * Notificações nativas (SO) + preferência do utilizador.
 * O backend Tauri implementa `app_show_notification`; em dev o macOS pode agrupar sob outro bundle (comportamento do plugin).
 */

import { invoke } from "@tauri-apps/api/core";

const STORAGE_KEY = "dcc:notificationsEnabled";
const DEFAULT_ENABLED = true;
const BODY_MAX_CHARS = 1800;

export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function areNotificationsEnabled(): boolean {
  if (typeof window === "undefined") return DEFAULT_ENABLED;
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored === null) return DEFAULT_ENABLED;
    return stored === "true";
  } catch {
    return DEFAULT_ENABLED;
  }
}

export function setNotificationsEnabled(enabled: boolean): void {
  try {
    localStorage.setItem(STORAGE_KEY, String(enabled));
  } catch {
    // ignore
  }
}

/** Limita o corpo para caber em banners do SO e no IPC. */
export function truncateNotificationBody(
  text: string,
  maxChars = BODY_MAX_CHARS
): string {
  const chars = [...text.trim()];
  if (chars.length <= maxChars) return chars.join("");
  return `${chars.slice(0, maxChars - 1).join("")}…`;
}

/**
 * Pede permissão ao SO (macOS/Windows/Linux conforme suportado pelo plugin).
 * Chamado ao ativar o toggle em Configurações; não falha a app se o utilizador recusar.
 */
export async function ensureOsNotificationPermission(): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  try {
    const { isPermissionGranted, requestPermission } = await import(
      "@tauri-apps/plugin-notification"
    );
    if (await isPermissionGranted()) return true;
    const next = await requestPermission();
    return next === "granted";
  } catch {
    return false;
  }
}

/**
 * Envia notificação nativa quando a preferência está ligada.
 * Supressão de ruído (mesmo pane visível) fica no hook de atenção do terminal.
 */
export async function showNativeNotification(
  title: string,
  body?: string
): Promise<{ ok: boolean }> {
  if (!areNotificationsEnabled()) return { ok: false };
  if (!isTauriRuntime()) return { ok: false };
  try {
    const result = await invoke<{ ok?: boolean; reason?: string }>(
      "app_show_notification",
      {
        payload: {
          title,
          body: body ? truncateNotificationBody(body) : undefined,
        },
      }
    );
    return { ok: result?.ok !== false };
  } catch {
    return { ok: false };
  }
}
