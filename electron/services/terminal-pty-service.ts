/**
 * Terminal PTY Service - Spawns pseudo-terminals for the embedded terminal (xterm.js).
 * Uses node-pty; main process only.
 */

import { platform } from "node:os";
import type { WebContents } from "electron";
import type { MissionAgentSession, PaneSession } from "../../lib/database/types";
import {
  excerptFromPlainTail,
  stripAnsiForAttention,
  tailLooksLikeWaitingForUser,
} from "../../lib/terminal/attention-heuristic";
import type { TerminalAttentionPayload } from "../../lib/terminal/attention-types";

export type { TerminalAttentionPayload } from "../../lib/terminal/attention-types";

const PANE_ATTENTION_THROTTLE_MS = 90_000;
const PANE_IDLE_SILENCE_MS = 50_000;

const paneAttentionLastEmit = new Map<string, number>();
const paneIdleTimers = new Map<string, ReturnType<typeof setTimeout>>();

function clearPaneIdleAttentionTimer(paneId: string): void {
  const t = paneIdleTimers.get(paneId);
  if (t) clearTimeout(t);
  paneIdleTimers.delete(paneId);
}

function schedulePaneIdleAttention(paneId: string, sender: WebContents): void {
  clearPaneIdleAttentionTimer(paneId);
  const t = setTimeout(() => {
    paneIdleTimers.delete(paneId);
    const session = paneSessions.get(paneId);
    if (!session || session.status !== "running") return;
    if (sender.isDestroyed()) return;
    maybeEmitPaneAttention(
      paneId,
      sender,
      "idle",
      undefined,
    );
  }, PANE_IDLE_SILENCE_MS);
  paneIdleTimers.set(paneId, t);
}

function maybeEmitPaneAttention(
  paneId: string,
  sender: WebContents,
  reason: "keyword" | "idle",
  excerpt: string | undefined,
): void {
  const now = Date.now();
  const last = paneAttentionLastEmit.get(paneId) ?? 0;
  if (now - last < PANE_ATTENTION_THROTTLE_MS) return;
  paneAttentionLastEmit.set(paneId, now);
  if (!sender.isDestroyed()) {
    const payload: TerminalAttentionPayload = {
      paneId,
      reason,
      ...(excerpt && excerpt.length > 0 ? { excerpt } : {}),
    };
    sender.send("terminal:attention", payload);
  }
}

function evaluatePaneOutputForAttention(
  paneId: string,
  plainPreview: string,
  sender: WebContents,
): void {
  if (tailLooksLikeWaitingForUser(plainPreview)) {
    const excerpt = excerptFromPlainTail(plainPreview);
    maybeEmitPaneAttention(paneId, sender, "keyword", excerpt);
  }
}

function teardownPaneAttention(paneId: string): void {
  clearPaneIdleAttentionTimer(paneId);
  paneAttentionLastEmit.delete(paneId);
}

/** Minimal type for node-pty spawn return (avoids importing node-pty types at compile time) */
interface PtyInstance {
  write(data: string): void;
  resize(cols: number, rows: number): void;
  kill(): void;
  onData(cb: (data: string) => void): void;
  onExit(cb: (e: { exitCode: number }) => void): void;
}

let ptyModule: {
  spawn: (
    file: string,
    args: string[],
    opts: {
      cwd: string;
      cols: number;
      rows: number;
      env: Record<string, string>;
      name: string;
    },
  ) => PtyInstance;
} | null = null;

function getPty(): NonNullable<typeof ptyModule> {
  if (!ptyModule) {
    try {
      ptyModule = require("node-pty");
    } catch {
      throw new Error("node-pty is not available. Run: yarn electron:rebuild");
    }
  }
  if (!ptyModule) throw new Error("node-pty is not available");
  return ptyModule;
}

const ptys = new Map<string, PtyInstance>();
/** missionId → ptyId for reattach; PTY is not killed on renderer unmount */
const missionIdToPtyId = new Map<string, string>();
/** ptyId → missionId for cleanup on exit */
const ptyIdToMissionId = new Map<string, string>();
/** paneId → ptyId for reattach (Comb/Pane architecture) */
const paneIdToPtyId = new Map<string, string>();
/** ptyId → paneId for cleanup on exit */
const ptyIdToPaneId = new Map<string, string>();
const MAX_OUTPUT_PREVIEW_CHARS = 100_000;

interface TerminalSessionState extends MissionAgentSession {
  ptyId: string;
  outputPreview: string;
}

export interface TerminalSessionEvent {
  type: "started" | "data" | "exit" | "killed";
  missionId?: string;
  ptyId: string;
  session: MissionAgentSession;
  chunk?: string;
  exitCode?: number | null;
}

const missionSessions = new Map<string, TerminalSessionState>();
const terminalSessionListeners = new Set<(event: TerminalSessionEvent) => void>();

function countLines(content: string): number {
  if (!content) return 0;
  return content.split(/\r?\n/).length;
}

function trimOutputPreview(content: string): string {
  if (content.length <= MAX_OUTPUT_PREVIEW_CHARS) return content;
  return content.slice(-MAX_OUTPUT_PREVIEW_CHARS);
}

function snapshotSession(
  session: TerminalSessionState | undefined | null,
): MissionAgentSession | null {
  if (!session) return null;
  return {
    ptyId: session.ptyId,
    cwd: session.cwd,
    command: session.command ?? null,
    args: [...(session.args ?? [])],
    status: session.status ?? "idle",
    startedAt: session.startedAt,
    lastActivityAt: session.lastActivityAt,
    exitedAt: session.exitedAt ?? null,
    lastExitCode: session.lastExitCode ?? null,
    outputPreview: session.outputPreview ?? "",
    outputLineCount: session.outputLineCount ?? countLines(session.outputPreview ?? ""),
  };
}

function emitTerminalSessionEvent(event: TerminalSessionEvent): void {
  for (const listener of terminalSessionListeners) {
    try {
      listener(event);
    } catch {
      // Ignore listener failures so PTY flow stays healthy.
    }
  }
}

function updateMissionSessionOutput(missionId: string, chunk: string): MissionAgentSession | null {
  const session = missionSessions.get(missionId);
  if (!session) return null;
  session.outputPreview = trimOutputPreview(`${session.outputPreview ?? ""}${chunk}`);
  session.outputLineCount = countLines(session.outputPreview);
  session.lastActivityAt = new Date().toISOString();
  return snapshotSession(session);
}

export function onTerminalSessionEvent(
  listener: (event: TerminalSessionEvent) => void,
): () => void {
  terminalSessionListeners.add(listener);
  return () => {
    terminalSessionListeners.delete(listener);
  };
}

export function getMissionSession(
  missionId: string,
): MissionAgentSession | null {
  return snapshotSession(missionSessions.get(missionId));
}

function defaultShell(): string {
  const plat = platform();
  if (plat === "win32") {
    return process.env.COMSPEC || "cmd.exe";
  }
  return process.env.SHELL || "/bin/bash";
}

export interface SpawnOptions {
  cwd: string;
  command?: string;
  args?: string[];
  cols?: number;
  rows?: number;
}

export interface SpawnResult {
  ptyId: string;
  error?: string;
  session?: MissionAgentSession | null;
}

/**
 * Spawn a PTY. If command is provided, runs that executable; otherwise runs the default shell.
 * Sends pty output to sender via "terminal:data" and "terminal:exit" events.
 * If missionId is set, registers the PTY so getOrCreate can return it for the same mission.
 */
export function spawnPty(
  options: SpawnOptions & { missionId?: string },
  sender: WebContents,
): SpawnResult {
  const pty = getPty();
  const { cwd, command, args = [], cols = 80, rows = 24, missionId } = options;

  try {
    const file = command ?? defaultShell();
    const argv = command ? args : [];
    const ptyProcess = pty.spawn(file, argv, {
      cwd,
      cols,
      rows,
      env: process.env as Record<string, string>,
      name: "xterm-256color",
    });

    const ptyId = `pty-${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
    ptys.set(ptyId, ptyProcess);
    const startedAt = new Date().toISOString();
    if (missionId) {
      missionIdToPtyId.set(missionId, ptyId);
      ptyIdToMissionId.set(ptyId, missionId);
      missionSessions.set(missionId, {
        ptyId,
        cwd,
        command: command ?? null,
        args,
        status: "running",
        startedAt,
        lastActivityAt: startedAt,
        exitedAt: null,
        lastExitCode: null,
        outputPreview: "",
        outputLineCount: 0,
      });
      const session = getMissionSession(missionId);
      if (session) {
        emitTerminalSessionEvent({
          type: "started",
          missionId,
          ptyId,
          session,
        });
      }
    }

    ptyProcess.onData((data: string) => {
      const mid = ptyIdToMissionId.get(ptyId);
      const session = mid ? updateMissionSessionOutput(mid, data) : null;
      if (mid && session) {
        emitTerminalSessionEvent({
          type: "data",
          missionId: mid,
          ptyId,
          session,
          chunk: data,
        });
      }
      if (!sender.isDestroyed()) {
        sender.send("terminal:data", ptyId, data);
      }
    });

    ptyProcess.onExit(({ exitCode }: { exitCode: number }) => {
      ptys.delete(ptyId);
      const mid = ptyIdToMissionId.get(ptyId);
      let session = mid ? missionSessions.get(mid) : undefined;
      if (session) {
        session.status = "exited";
        session.lastExitCode = exitCode;
        session.exitedAt = new Date().toISOString();
        session.lastActivityAt = session.exitedAt;
      }
      if (mid) {
        ptyIdToMissionId.delete(ptyId);
        missionIdToPtyId.delete(mid);
      }
      if (mid && session) {
        emitTerminalSessionEvent({
          type: "exit",
          missionId: mid,
          ptyId,
          session: snapshotSession(session)!,
          exitCode,
        });
      }
      if (!sender.isDestroyed()) {
        sender.send("terminal:exit", ptyId, exitCode);
      }
    });

    return { ptyId, session: missionId ? getMissionSession(missionId) : null };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return { ptyId: "", error: message };
  }
}

export interface GetOrCreateOptions extends SpawnOptions {
  missionId: string;
}

/**
 * Return existing ptyId for missionId if the PTY is still alive; otherwise spawn a new PTY
 * and register it for this missionId. Used so the renderer can reattach when navigating back.
 */
export function getOrCreatePty(
  options: GetOrCreateOptions,
  sender: WebContents,
): SpawnResult {
  const { missionId, cwd, command, args, cols, rows } = options;
  const existingPtyId = missionIdToPtyId.get(missionId);
  if (existingPtyId && ptys.has(existingPtyId)) {
    return { ptyId: existingPtyId, session: getMissionSession(missionId) };
  }
  if (existingPtyId) {
    missionIdToPtyId.delete(missionId);
    ptyIdToMissionId.delete(existingPtyId);
  }
  return spawnPty(
    { cwd, command, args, cols, rows, missionId },
    sender,
  );
}

export function writeToPty(ptyId: string, data: string): boolean {
  const p = ptys.get(ptyId);
  if (!p) return false;
  try {
    p.write(data);
    return true;
  } catch {
    return false;
  }
}

export function resizePty(ptyId: string, cols: number, rows: number): boolean {
  const p = ptys.get(ptyId);
  if (!p) return false;
  try {
    p.resize(cols, rows);
    return true;
  } catch {
    return false;
  }
}

export function killPty(ptyId: string): boolean {
  const p = ptys.get(ptyId);
  if (!p) return false;
  try {
    p.kill();
    ptys.delete(ptyId);
    const mid = ptyIdToMissionId.get(ptyId);
    const session = mid ? missionSessions.get(mid) : undefined;
    if (session) {
      session.status = "exited";
      session.exitedAt = new Date().toISOString();
      session.lastActivityAt = session.exitedAt;
    }
    if (mid) {
      ptyIdToMissionId.delete(ptyId);
      missionIdToPtyId.delete(mid);
    }
    if (mid && session) {
      emitTerminalSessionEvent({
        type: "killed",
        missionId: mid,
        ptyId,
        session: snapshotSession(session)!,
        exitCode: session.lastExitCode ?? null,
      });
    }
    return true;
  } catch {
    return false;
  }
}

/**
 * Kill the PTY associated with a mission (e.g. when user discards worktree).
 */
export function killPtyByMissionId(missionId: string): boolean {
  const ptyId = missionIdToPtyId.get(missionId);
  if (!ptyId) return false;
  return killPty(ptyId);
}

// ==========================================
// Pane-based PTY management (Comb/Pane arch)
// ==========================================

interface PaneSessionState extends PaneSession {
  ptyId: string;
  outputPreview: string;
}

const paneSessions = new Map<string, PaneSessionState>();

function snapshotPaneSession(
  session: PaneSessionState | undefined | null,
): PaneSession | null {
  if (!session) return null;
  return {
    ptyId: session.ptyId,
    cwd: session.cwd,
    command: session.command ?? null,
    args: [...(session.args ?? [])],
    status: session.status ?? "idle",
    startedAt: session.startedAt,
    lastActivityAt: session.lastActivityAt,
    exitedAt: session.exitedAt ?? null,
    lastExitCode: session.lastExitCode ?? null,
    outputPreview: session.outputPreview ?? "",
    outputLineCount: session.outputLineCount ?? countLines(session.outputPreview ?? ""),
  };
}

export function getPaneSession(paneId: string): PaneSession | null {
  return snapshotPaneSession(paneSessions.get(paneId));
}

export interface GetOrCreatePaneOptions extends SpawnOptions {
  paneId: string;
  /** When true, spawn a new process even if the last pane session exited (clears exited snapshot). */
  restart?: boolean;
}

export interface PaneSpawnResult {
  ptyId?: string;
  error?: string;
  session?: PaneSession | null;
}

export function getOrCreatePtyForPane(
  options: GetOrCreatePaneOptions,
  sender: WebContents,
): PaneSpawnResult {
  const { paneId, cwd, command, args, cols, rows, restart } = options;
  const existingPtyId = paneIdToPtyId.get(paneId);
  if (existingPtyId && ptys.has(existingPtyId)) {
    return { ptyId: existingPtyId, session: getPaneSession(paneId) };
  }
  if (existingPtyId) {
    paneIdToPtyId.delete(paneId);
    ptyIdToPaneId.delete(existingPtyId);
  }

  if (restart) {
    paneSessions.delete(paneId);
  } else {
    const sess = paneSessions.get(paneId);
    if (sess?.status === "exited") {
      return { session: getPaneSession(paneId) };
    }
  }

  const pty = getPty();
  try {
    const file = command ?? defaultShell();
    const argv = command ? (args ?? []) : [];
    const ptyProcess = pty.spawn(file, argv, {
      cwd,
      cols: cols ?? 80,
      rows: rows ?? 24,
      env: process.env as Record<string, string>,
      name: "xterm-256color",
    });

    const ptyId = `pty-${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
    ptys.set(ptyId, ptyProcess);
    paneIdToPtyId.set(paneId, ptyId);
    ptyIdToPaneId.set(ptyId, paneId);

    const startedAt = new Date().toISOString();
    paneSessions.set(paneId, {
      ptyId,
      cwd,
      command: command ?? null,
      args: args ?? [],
      status: "running",
      startedAt,
      lastActivityAt: startedAt,
      exitedAt: null,
      lastExitCode: null,
      outputPreview: "",
      outputLineCount: 0,
    });

    ptyProcess.onData((data: string) => {
      const pid = ptyIdToPaneId.get(ptyId);
      if (pid) {
        const session = paneSessions.get(pid);
        if (session) {
          session.outputPreview = trimOutputPreview(`${session.outputPreview ?? ""}${data}`);
          session.outputLineCount = countLines(session.outputPreview);
          session.lastActivityAt = new Date().toISOString();
          const plain = stripAnsiForAttention(session.outputPreview);
          evaluatePaneOutputForAttention(pid, plain, sender);
          schedulePaneIdleAttention(pid, sender);
        }
      }
      if (!sender.isDestroyed()) {
        sender.send("terminal:data", ptyId, data);
      }
    });

    ptyProcess.onExit(({ exitCode }: { exitCode: number }) => {
      ptys.delete(ptyId);
      const pid = ptyIdToPaneId.get(ptyId);
      if (pid) {
        teardownPaneAttention(pid);
        const session = paneSessions.get(pid);
        if (session) {
          session.status = "exited";
          session.lastExitCode = exitCode;
          session.exitedAt = new Date().toISOString();
          session.lastActivityAt = session.exitedAt;
        }
        ptyIdToPaneId.delete(ptyId);
        paneIdToPtyId.delete(pid);
      }
      if (!sender.isDestroyed()) {
        sender.send("terminal:exit", ptyId, exitCode);
      }
    });

    return { ptyId, session: getPaneSession(paneId) };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return { error: message };
  }
}

export function killPtyByPaneId(paneId: string): boolean {
  const ptyId = paneIdToPtyId.get(paneId);
  if (!ptyId) return false;
  return killPty(ptyId);
}
