/**
 * Native notifications helper.
 * Shows OS notification when user has enabled them (localStorage dcc:notificationsEnabled).
 */

const STORAGE_KEY = "dcc:notificationsEnabled";
const DEFAULT_ENABLED = true;

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

export function showNativeNotification(
  title: string,
  body?: string
): void {
  if (!areNotificationsEnabled()) return;
  try {
    window.electronAPI?.app?.showNotification?.({ title, body });
  } catch {
    // ignore
  }
}
