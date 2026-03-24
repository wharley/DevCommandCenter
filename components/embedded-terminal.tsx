"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Terminal as XTerm } from "xterm";
import { FitAddon } from "@xterm/addon-fit";
import "xterm/css/xterm.css";
import { Button } from "@/components/ui/button";
import { X } from "lucide-react";
import type { TerminalAttentionPayload } from "@/lib/terminal/attention-types";

export interface EmbeddedTerminalProps {
  cwd: string;
  command?: string;
  args?: string[];
  onClose?: () => void;
  onExit?: (code: number) => void;
  /** Optional label for the terminal */
  title?: string;
  /**
   * When set, PTY is keyed by paneId (Workspace architecture): getOrCreateForPane is used
   * and the PTY is NOT killed on unmount for reattach.
   */
  paneId?: string;
}

type PaneAttachResult = {
  ptyId?: string;
  error?: string;
  session?: any | null;
};

export function EmbeddedTerminal({
  cwd,
  command,
  args = [],
  onClose,
  onExit,
  title,
  paneId,
}: EmbeddedTerminalProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const xtermRef = useRef<XTerm | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const ptyIdRef = useRef<string | null>(null);
  const onExitRef = useRef<typeof onExit>(onExit);
  const [error, setError] = useState<string | null>(null);
  const [exited, setExited] = useState<number | null>(null);
  const [isWaiting, setIsWaiting] = useState(false);
  /** Pane agent: mostrar reinício quando o processo terminou ou só há sessão encerrada no main. */
  const [agentCanRestart, setAgentCanRestart] = useState(false);
  const argsKey = useMemo(() => JSON.stringify(args), [args]);
  const stableArgs = useMemo(() => args, [argsKey]);
  const shellCommand = useMemo(() => {
    const fromEnv = (window as unknown as { __DCC_SHELL__?: string }).__DCC_SHELL__;
    if (fromEnv && fromEnv.trim()) return fromEnv;
    return window.desktopAPI?.platform === "win32" ? "powershell" : "/bin/zsh";
  }, []);

  useEffect(() => {
    onExitRef.current = onExit;
  }, [onExit]);

  const safeWrite = useCallback((term: XTerm, chunk: string) => {
    if (!chunk || xtermRef.current !== term) return;
    try {
      term.write(chunk);
    } catch {
      // xterm may throw in teardown races; ignore stale writes
    }
  }, []);

  /** Reidrata buffer de sessão (Tauri) antes de assinar onData, evitando duplicar linhas. */
  const hydrateTerminalBacklog = useCallback(async (term: XTerm, ptyId: string | undefined) => {
    const tapi = window.desktopAPI?.terminal;
    if (!ptyId || !tapi?.getBacklog) return;
    try {
      const r = await tapi.getBacklog(ptyId);
      const lines = r?.lines;
      if (lines?.length && xtermRef.current === term) safeWrite(term, lines.join(""));
    } catch {
      /* ignore */
    }
  }, [safeWrite]);

  const applyPaneAttachResult = useCallback(
    (result: PaneAttachResult, term: XTerm) => {
      if (xtermRef.current !== term) return;
      if (result.error) {
        setError(result.error);
        return;
      }
      setError(null);
      if (result.ptyId) {
        ptyIdRef.current = result.ptyId;
        setAgentCanRestart(false);
        setExited(null);
      } else if (result.session?.status === "exited" && command) {
        setAgentCanRestart(true);
      }
      if (result.session?.outputPreview) {
        safeWrite(term, result.session.outputPreview);
      }
      if (
        !result.ptyId &&
        typeof result.session?.lastExitCode === "number"
      ) {
        setExited(result.session.lastExitCode);
      }
    },
    [command, safeWrite],
  );

  const handleRestartAgent = useCallback(() => {
    const api = window.desktopAPI?.terminal;
    const term = xtermRef.current;
    if (!api?.getOrCreateForPane || !paneId || !term) return;
    term.reset();
    void api
      .getOrCreateForPane(paneId, {
        cwd,
        command,
        args: stableArgs,
        cols: term.cols,
        rows: term.rows,
        restart: true,
      })
      .then(async (result) => {
        if (xtermRef.current !== term) return;
        applyPaneAttachResult(result, term);
        await hydrateTerminalBacklog(term, result.ptyId);
      });
  }, [applyPaneAttachResult, hydrateTerminalBacklog, paneId, cwd, command, stableArgs]);

  const killPty = useCallback(() => {
    const id = ptyIdRef.current;
    if (id && window.desktopAPI?.terminal?.kill) {
      window.desktopAPI.terminal.kill(id);
      ptyIdRef.current = null;
    }
  }, []);

  const handleClose = useCallback(() => {
    killPty();
    onClose?.();
  }, [killPty, onClose]);

  useEffect(() => {
    const api = window.desktopAPI?.terminal;
    if (!api || !containerRef.current) return;
    let disposed = false;
    let resizeRaf: number | null = null;

    const term = new XTerm({
      cursorBlink: true,
      fontSize: 13,
      fontFamily: "var(--font-geist-mono, 'Menlo', monospace)",
      theme: {
        background: "#1a1a1a",
        foreground: "#e5e5e5",
        cursor: "#e5e5e5",
        black: "#1e1e1e",
        red: "#cd3131",
        green: "#0dbc79",
        yellow: "#e5e510",
        blue: "#2472c8",
        magenta: "#bc3fbc",
        cyan: "#11a8cd",
        white: "#e5e5e5",
      },
    });

    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(containerRef.current);
    xtermRef.current = term;
    fitAddonRef.current = fitAddon;
    const safeFit = () => {
      if (disposed || xtermRef.current !== term) return;
      const el = containerRef.current;
      // fit() before layout (0×0 flex) or with a torn-down renderer can race xterm's Viewport RAF
      if (!el || el.offsetWidth < 1 || el.offsetHeight < 1) return;
      try {
        fitAddon.fit();
      } catch {
        // xterm can throw during unmount/rerender races; ignore and wait next resize tick
      }
    };
    // Defer until after flex/min-h-0 layout so FitAddon sees real cell metrics
    requestAnimationFrame(() => {
      requestAnimationFrame(safeFit);
    });

    let unsubData: (() => void) | null = null;

    const unsubExit = api.onExit((id: string, code: number) => {
      if (id === ptyIdRef.current) {
        ptyIdRef.current = null;
        setExited(code);
        setIsWaiting(false);
        if (paneId && command) setAgentCanRestart(true);
        onExitRef.current?.(code);
      }
    });

    const unsubAttention = api.onAttention?.((payload: TerminalAttentionPayload) => {
      const id = payload.ptyId ?? payload.paneId;
      const status = payload.status ?? (payload.reason === "idle" ? "idle" : "waiting");
      if (id === ptyIdRef.current) {
        setIsWaiting(status === "waiting");
      }
    });

    term.onData((data: string) => {
      const id = ptyIdRef.current;
      if (id) {
        api.write(id, data);
        setIsWaiting(false); // Reset waiting state on user input
      }
    });

    const spawnOptions =
      command && command.trim().length > 0
        ? {
            cwd,
            command: shellCommand,
            args: ["-il", "-c", `${command} ${stableArgs.join(" ")}`.trim()],
            cols: term.cols,
            rows: term.rows,
          }
        : {
            cwd,
            command: shellCommand,
            args: ["-il"],
            cols: term.cols,
            rows: term.rows,
          };

    const mountSession = async () => {
      try {
        if (paneId && api.getOrCreateForPane) {
          const result = await api.getOrCreateForPane(paneId, spawnOptions);
          if (disposed || xtermRef.current !== term) return;
          applyPaneAttachResult(result, term);
          await hydrateTerminalBacklog(term, result.ptyId);
        } else {
          const result = await api.spawn(spawnOptions);
          if (disposed || xtermRef.current !== term) return;
          if (result.error) {
            setError(result.error);
            return;
          }
          if (result.ptyId) ptyIdRef.current = result.ptyId;
          await hydrateTerminalBacklog(term, result.ptyId);
        }
        unsubData = api.onData((id: string, data: string) => {
          if (id === ptyIdRef.current && !disposed && xtermRef.current === term) {
            safeWrite(term, data);
          }
        });
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      }
    };

    void mountSession();

    const handleResize = () => {
      if (disposed || xtermRef.current !== term) return;
      if (fitAddonRef.current && ptyIdRef.current) {
        safeFit();
        const { cols, rows } = term;
        if (cols && rows) {
          api.resize(ptyIdRef.current, cols, rows);
        }
      }
    };

    const resizeObserver = new ResizeObserver(() => {
      if (resizeRaf !== null) cancelAnimationFrame(resizeRaf);
      resizeRaf = requestAnimationFrame(handleResize);
    });
    resizeObserver.observe(containerRef.current);

    return () => {
      disposed = true;
      if (resizeRaf !== null) cancelAnimationFrame(resizeRaf);
      resizeObserver.disconnect();
      unsubData?.();
      unsubExit();
      unsubAttention?.();
      const id = ptyIdRef.current;
      // When paneId is set, do NOT kill on unmount so the user can reattach
      if (id && api.kill && !paneId) {
        api.kill(id);
      }
      ptyIdRef.current = null;
      xtermRef.current = null;
      fitAddonRef.current = null;
      // Defer dispose so pending xterm Viewport/Render RAFs finish before the renderer is cleared
      // (otherwise: TypeError on this._renderer.value.dimensions).
      const t = term;
      setTimeout(() => {
        try {
          t.dispose();
        } catch {
          /* ignore */
        }
      }, 0);
    };
  }, [
    applyPaneAttachResult,
    command,
    cwd,
    hydrateTerminalBacklog,
    paneId,
    safeWrite,
    stableArgs,
  ]);

  return (
    <div className={`flex h-full min-h-0 min-w-0 flex-col overflow-hidden rounded-lg border transition-all duration-300 ${
      isWaiting ? "border-primary shadow-[0_0_15px_rgba(var(--primary),0.3)] ring-1 ring-primary" : "border-border"
    } bg-background`}>
      <div className={`flex items-center justify-between border-b px-2 py-1.5 ${
        isWaiting ? "border-primary bg-primary/5" : "border-border"
      }`}>
        <span className="truncate text-xs text-muted-foreground">
          {title ?? "Terminal"} — {cwd}
        </span>
        <div className="flex items-center gap-1">
          {paneId && command && agentCanRestart && (
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="h-7 text-xs"
              onClick={handleRestartAgent}
            >
              Reiniciar agente
            </Button>
          )}
          {exited !== null && (
            <span className="text-xs text-muted-foreground">
              (saiu com código {exited})
            </span>
          )}
          {onClose && (
            <Button
              variant="ghost"
              size="icon"
              className="h-7 w-7"
              onClick={handleClose}
              aria-label="Fechar terminal"
            >
              <X className="h-4 w-4" />
            </Button>
          )}
        </div>
      </div>
      {error && (
        <div className="border-b border-border bg-destructive/10 px-2 py-1.5 text-sm text-destructive">
          {error}
        </div>
      )}
      <div
        ref={containerRef}
        className="min-h-0 flex-1 overflow-hidden p-1"
        style={{ height: "100%" }}
      />
    </div>
  );
}
