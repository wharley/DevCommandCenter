/**
 * Métricas opcionais de throughput do stream terminal → xterm (dev).
 * Ative: localStorage.setItem('dcc.debugTerminalMetrics', '1') e recarregue.
 * Desative: removeItem ou '0'.
 */

import {
  loadTerminalOutputLagPreferences,
} from "@/lib/terminal/output-lag-preferences";

export type TerminalOutputLagReason = "pending-writes" | "render-latency" | "throughput";

export interface TerminalOutputHealthSnapshot {
  bytesPerSecond: number;
  chunksPerSecond: number;
  pendingWrites: number;
  avgWriteLatencyMs: number | null;
  maxWriteLatencyMs: number;
  isSlowLink: boolean;
  reason: TerminalOutputLagReason | null;
}

export interface TerminalOutputMetricsTracker {
  beginChunk(bytes: number): () => void;
  getSnapshot(): TerminalOutputHealthSnapshot;
  subscribe(listener: (snapshot: TerminalOutputHealthSnapshot) => void): () => void;
  dispose(): void;
}

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

const WINDOW_MS = 1000;
const IDLE_CLEAR_MS = 1400;
const MIN_BYTES_PER_SECOND = 192 * 1024;
const HIGH_BYTES_PER_SECOND = 512 * 1024;
const VERY_HIGH_BYTES_PER_SECOND = 768 * 1024;
const MIN_AVG_LATENCY_MS = 18;
const HIGH_AVG_LATENCY_MS = 28;
const HIGH_MAX_LATENCY_MS = 48;
const PENDING_WRITE_THRESHOLD = 2;
const HIGH_PENDING_WRITE_THRESHOLD = 4;

type ByteSample = { at: number; bytes: number };
type LatencySample = { at: number; latencyMs: number };

function createBaselineSnapshot(): TerminalOutputHealthSnapshot {
  return {
    bytesPerSecond: 0,
    chunksPerSecond: 0,
    pendingWrites: 0,
    avgWriteLatencyMs: null,
    maxWriteLatencyMs: 0,
    isSlowLink: false,
    reason: null,
  };
}

function snapshotChanged(
  a: TerminalOutputHealthSnapshot,
  b: TerminalOutputHealthSnapshot,
): boolean {
  return (
    a.bytesPerSecond !== b.bytesPerSecond ||
    a.chunksPerSecond !== b.chunksPerSecond ||
    a.pendingWrites !== b.pendingWrites ||
    a.avgWriteLatencyMs !== b.avgWriteLatencyMs ||
    a.maxWriteLatencyMs !== b.maxWriteLatencyMs ||
    a.isSlowLink !== b.isSlowLink ||
    a.reason !== b.reason
  );
}

function roundBytesPerSecond(value: number): number {
  return Math.round(value / 1024) * 1024;
}

function roundLatency(value: number | null): number | null {
  return value === null ? null : Math.round(value * 10) / 10;
}

function scaleThreshold(value: number, sensitivity: number): number {
  return value * sensitivity;
}

function scaleCountThreshold(value: number, sensitivity: number): number {
  return Math.max(1, Math.round(value * sensitivity));
}

export function createTerminalOutputMetricsTracker(): TerminalOutputMetricsTracker {
  if (typeof window === "undefined") {
    const noopSnapshot = createBaselineSnapshot();
    return {
      beginChunk: () => () => {},
      getSnapshot: () => noopSnapshot,
      subscribe: () => () => {},
      dispose: () => {},
    };
  }

  let disposed = false;
  let pendingWrites = 0;
  let slowLink = false;
  let slowStreak = 0;
  let calmStreak = 0;
  let idleTimer: number | null = null;
  let rafId: number | null = null;
  let lastActivityAt = window.performance.now();
  let snapshot = createBaselineSnapshot();

  const byteSamples: ByteSample[] = [];
  const latencySamples: LatencySample[] = [];
  const listeners = new Set<(snapshot: TerminalOutputHealthSnapshot) => void>();
  const onLagPrefsChanged = () => scheduleCompute();

  const pruneOldSamples = (now: number) => {
    while (byteSamples.length > 0 && now - byteSamples[0].at > WINDOW_MS) {
      byteSamples.shift();
    }
    while (latencySamples.length > 0 && now - latencySamples[0].at > WINDOW_MS) {
      latencySamples.shift();
    }
  };

  const notify = (next: TerminalOutputHealthSnapshot) => {
    if (!snapshotChanged(snapshot, next)) return;
    snapshot = next;
    listeners.forEach((listener) => listener(snapshot));
  };

  const computeSnapshot = (now: number, forceClear = false) => {
    pruneOldSamples(now);
    const sensitivity = loadTerminalOutputLagPreferences().sensitivity;

    const hasSamples = byteSamples.length > 0 || latencySamples.length > 0 || pendingWrites > 0;
    const shouldClear = forceClear && pendingWrites === 0;
    if (!hasSamples || shouldClear) {
      pendingWrites = 0;
      slowLink = false;
      slowStreak = 0;
      calmStreak = 0;
      notify(createBaselineSnapshot());
      return;
    }

    const oldestSampleAt = Math.min(
      byteSamples[0]?.at ?? now,
      latencySamples[0]?.at ?? now,
      lastActivityAt,
    );
    const elapsedMs = Math.max(now - oldestSampleAt, 1);

    const totalBytes = byteSamples.reduce((acc, sample) => acc + sample.bytes, 0);
    const totalChunks = byteSamples.length;
    const totalLatency = latencySamples.reduce((acc, sample) => acc + sample.latencyMs, 0);
    const maxLatency = latencySamples.reduce((acc, sample) => Math.max(acc, sample.latencyMs), 0);
    const avgLatency = latencySamples.length > 0 ? totalLatency / latencySamples.length : null;
    const bytesPerSecond = (totalBytes * 1000) / elapsedMs;
    const chunksPerSecond = (totalChunks * 1000) / elapsedMs;

    const pendingPressure =
      pendingWrites >= scaleCountThreshold(HIGH_PENDING_WRITE_THRESHOLD, sensitivity) ||
      (pendingWrites >= scaleCountThreshold(PENDING_WRITE_THRESHOLD, sensitivity) &&
        (avgLatency ?? 0) >= scaleThreshold(MIN_AVG_LATENCY_MS, sensitivity));
    const renderPressure =
      (avgLatency !== null && avgLatency >= scaleThreshold(HIGH_AVG_LATENCY_MS, sensitivity)) ||
      maxLatency >= scaleThreshold(HIGH_MAX_LATENCY_MS, sensitivity);
    const throughputPressure =
      bytesPerSecond >= scaleThreshold(HIGH_BYTES_PER_SECOND, sensitivity) ||
      (bytesPerSecond >= scaleThreshold(MIN_BYTES_PER_SECOND, sensitivity) &&
        chunksPerSecond >= scaleCountThreshold(12, sensitivity));

    const rawSlow = pendingPressure || renderPressure || throughputPressure;
    if (rawSlow) {
      slowStreak += 1;
      calmStreak = 0;
      if (
        slowStreak >= 2 ||
        pendingWrites >= scaleCountThreshold(HIGH_PENDING_WRITE_THRESHOLD, sensitivity) ||
        bytesPerSecond >= scaleThreshold(VERY_HIGH_BYTES_PER_SECOND, sensitivity) ||
        maxLatency >= scaleThreshold(HIGH_MAX_LATENCY_MS, sensitivity)
      ) {
        slowLink = true;
      }
    } else {
      calmStreak += 1;
      slowStreak = 0;
      if (slowLink && calmStreak >= 3 && pendingWrites === 0) {
        slowLink = false;
      }
    }

    const reason: TerminalOutputLagReason | null = slowLink
      ? pendingPressure
        ? "pending-writes"
        : renderPressure
          ? "render-latency"
          : throughputPressure
            ? "throughput"
            : null
      : null;

    notify({
      bytesPerSecond: roundBytesPerSecond(bytesPerSecond),
      chunksPerSecond: Math.round(chunksPerSecond * 10) / 10,
      pendingWrites,
      avgWriteLatencyMs: roundLatency(avgLatency),
      maxWriteLatencyMs: Math.round(maxLatency * 10) / 10,
      isSlowLink: slowLink,
      reason,
    });
  };

  const scheduleCompute = (forceClear = false) => {
    if (disposed) return;
    if (rafId !== null) {
      if (forceClear) {
        window.cancelAnimationFrame(rafId);
        rafId = null;
      } else {
        return;
      }
    }

    rafId = window.requestAnimationFrame((ts) => {
      rafId = null;
      computeSnapshot(ts, forceClear);
    });
  };

  const resetIdleTimer = () => {
    if (idleTimer !== null) {
      window.clearTimeout(idleTimer);
    }
    idleTimer = window.setTimeout(() => {
      if (disposed) return;
      if (pendingWrites > 0) {
        scheduleCompute();
        resetIdleTimer();
        return;
      }
      scheduleCompute(true);
    }, IDLE_CLEAR_MS);
  };

  window.addEventListener("dcc-terminal-output-lag-prefs-changed", onLagPrefsChanged);

  return {
    beginChunk(bytes: number) {
      if (disposed) return () => {};
      const startedAt = window.performance.now();
      lastActivityAt = startedAt;
      byteSamples.push({ at: startedAt, bytes });
      pendingWrites += 1;
      scheduleCompute();
      resetIdleTimer();

      let finished = false;
      return () => {
        if (disposed || finished) return;
        finished = true;
        const now = window.performance.now();
        lastActivityAt = now;
        pendingWrites = Math.max(0, pendingWrites - 1);
        latencySamples.push({ at: now, latencyMs: now - startedAt });
        scheduleCompute();
        resetIdleTimer();
      };
    },
    getSnapshot: () => snapshot,
    subscribe(listener) {
      listeners.add(listener);
      listener(snapshot);
      return () => {
        listeners.delete(listener);
      };
    },
    dispose() {
      disposed = true;
      window.removeEventListener("dcc-terminal-output-lag-prefs-changed", onLagPrefsChanged);
      if (rafId !== null) {
        window.cancelAnimationFrame(rafId);
        rafId = null;
      }
      if (idleTimer !== null) {
        window.clearTimeout(idleTimer);
        idleTimer = null;
      }
      listeners.clear();
      byteSamples.length = 0;
      latencySamples.length = 0;
    },
  };
}
