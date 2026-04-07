/**
 * Métricas opcionais de throughput do stream terminal → xterm (dev).
 * Ative: localStorage.setItem('dcc.debugTerminalMetrics', '1') e recarregue.
 * Desative: removeItem ou '0'.
 */

let bytesWindow = 0;
let lastLog = 0;
let rafScheduled = false;

export function recordTerminalOutputBytes(n: number): void {
  if (typeof window === "undefined") return;
  try {
    if (localStorage.getItem("dcc.debugTerminalMetrics") !== "1") return;
  } catch {
    return;
  }
  bytesWindow += n;
  if (rafScheduled) return;
  rafScheduled = true;
  requestAnimationFrame(() => {
    rafScheduled = false;
    const now = performance.now();
    if (now - lastLog < 1000) return;
    lastLog = now;
    const kb = (bytesWindow / 1024).toFixed(1);
    bytesWindow = 0;
    // eslint-disable-next-line no-console -- intentional dev metric
    console.info(`[dcc:terminal-metrics] ~${kb} KiB/s para o xterm (amostra 1s)`);
  });
}
