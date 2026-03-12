"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { Terminal as XTerm } from "xterm";
import { FitAddon } from "@xterm/addon-fit";
import "xterm/css/xterm.css";
import { Button } from "@/components/ui/button";
import { X } from "lucide-react";

export interface EmbeddedTerminalProps {
  cwd: string;
  command?: string;
  args?: string[];
  onClose?: () => void;
  onExit?: (code: number) => void;
  /** Optional label for the terminal (e.g. mission title) */
  title?: string;
  /**
   * When set, PTY is keyed by missionId: getOrCreate is used and the PTY is NOT killed on unmount,
   * so the user can navigate away and reattach to the same session later.
   */
  missionId?: string;
}

export function EmbeddedTerminal({
  cwd,
  command,
  args = [],
  onClose,
  onExit,
  title,
  missionId,
}: EmbeddedTerminalProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const xtermRef = useRef<XTerm | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const ptyIdRef = useRef<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [exited, setExited] = useState<number | null>(null);

  const killPty = useCallback(() => {
    const id = ptyIdRef.current;
    if (id && window.electronAPI?.terminal?.kill) {
      window.electronAPI.terminal.kill(id);
      ptyIdRef.current = null;
    }
  }, []);

  const handleClose = useCallback(() => {
    killPty();
    onClose?.();
  }, [killPty, onClose]);

  useEffect(() => {
    const api = window.electronAPI?.terminal;
    if (!api || !containerRef.current) return;

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
    fitAddon.fit();

    xtermRef.current = term;
    fitAddonRef.current = fitAddon;

    const unsubData = api.onData((id: string, data: string) => {
      if (id === ptyIdRef.current) term.write(data);
    });
    const unsubExit = api.onExit((id: string, code: number) => {
      if (id === ptyIdRef.current) {
        ptyIdRef.current = null;
        setExited(code);
        onExit?.(code);
      }
    });

    term.onData((data: string) => {
      const id = ptyIdRef.current;
      if (id) api.write(id, data);
    });

    const spawnOptions = {
      cwd,
      command,
      args,
      cols: term.cols,
      rows: term.rows,
    };

    if (missionId && api.getOrCreate) {
      api.getOrCreate(missionId, spawnOptions).then((result) => {
        if (result.error) {
          setError(result.error);
          return;
        }
        if (result.ptyId) ptyIdRef.current = result.ptyId;
      });
    } else {
      api.spawn(spawnOptions).then((result) => {
        if (result.error) {
          setError(result.error);
          return;
        }
        if (result.ptyId) ptyIdRef.current = result.ptyId;
      });
    }

    const resizeObserver = new ResizeObserver(() => {
      fitAddon.fit();
      const id = ptyIdRef.current;
      if (id && term.cols && term.rows) {
        api.resize(id, term.cols, term.rows);
      }
    });
    resizeObserver.observe(containerRef.current);

    return () => {
      resizeObserver.disconnect();
      unsubData();
      unsubExit();
      const id = ptyIdRef.current;
      // When missionId is set, do NOT kill on unmount so the user can reattach when navigating back
      if (id && api.kill && !missionId) {
        api.kill(id);
      }
      ptyIdRef.current = null;
      term.dispose();
      xtermRef.current = null;
      fitAddonRef.current = null;
    };
  }, [args, command, cwd, missionId, onExit]);

  return (
    <div className="flex h-full flex-col rounded-lg border border-border bg-background">
      <div className="flex items-center justify-between border-b border-border px-2 py-1.5">
        <span className="truncate text-xs text-muted-foreground">
          {title ?? "Terminal"} — {cwd}
        </span>
        <div className="flex items-center gap-1">
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
