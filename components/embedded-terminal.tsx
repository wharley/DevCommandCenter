"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Terminal as XTerm } from "xterm";
import { FitAddon } from "@xterm/addon-fit";
import "xterm/css/xterm.css";
import { Button } from "@/components/ui/button";
import { X, Paperclip, ArrowDown } from "lucide-react";
import { useTheme } from "@/components/theme-provider";
import {
  type TerminalAttentionPayload,
  resolveAttentionPhase,
} from "@/lib/terminal/attention-types";
import { recordTerminalOutputBytes } from "@/lib/terminal/output-metrics";
import { loadTerminalAppearance } from "@/lib/terminal/terminal-preferences";
import { getXtermColorTheme } from "@/lib/terminal/xterm-theme";

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
  /**
   * Auto-start the process on mount. Default: true.
   * Set to false for agent panes to avoid token consumption on app restart.
   */
  autoStart?: boolean;
  /**
   * Callback to notify parent that this terminal is ready but not started.
   */
  onReadyNotStarted?: () => void;
  /**
   * Callback when parent wants to start the agent manually.
   * Only applicable when autoStart=false.
   */
  onStartRequest?: (startFn: () => void) => void;
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
  autoStart = true,
  onReadyNotStarted,
  onStartRequest,
}: EmbeddedTerminalProps) {
  const { resolvedTheme } = useTheme();
  const [terminalPrefsEpoch, setTerminalPrefsEpoch] = useState(0);
  const containerRef = useRef<HTMLDivElement>(null);
  const xtermRef = useRef<XTerm | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const ptyIdRef = useRef<string | null>(null);
  const onExitRef = useRef<typeof onExit>(onExit);
  /** Rastreia se usuário está no final do terminal para auto-scroll inteligente */
  const isAtBottomRef = useRef(true);
  /** Rastreia estado de waiting para evitar re-renders desnecessários */
  const isWaitingRef = useRef(false);
  /** Contador de output novo quando usuário não está no final */
  const [hasNewOutput, setHasNewOutput] = useState(false);
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

  useEffect(() => {
    const onPrefs = () => setTerminalPrefsEpoch((n) => n + 1);
    window.addEventListener("dcc-terminal-prefs-changed", onPrefs);
    return () => window.removeEventListener("dcc-terminal-prefs-changed", onPrefs);
  }, []);

  useEffect(() => {
    const onAction = (ev: Event) => {
      const ce = ev as CustomEvent<{ type?: string }>;
      if (ce.detail?.type !== "clearScrollback") return;
      const t = xtermRef.current;
      if (t) t.clear();
    };
    window.addEventListener("dcc-terminal-action", onAction as EventListener);
    return () => window.removeEventListener("dcc-terminal-action", onAction as EventListener);
  }, []);

  useEffect(() => {
    const term = xtermRef.current;
    if (!term) return;
    const prefs = loadTerminalAppearance();
    term.options.fontSize = prefs.fontSize;
    term.options.fontFamily = prefs.fontFamily;
    term.options.theme = getXtermColorTheme(resolvedTheme, prefs.useAppThemeColors);
    requestAnimationFrame(() => {
      try {
        fitAddonRef.current?.fit();
      } catch {
        /* ignore */
      }
    });
  }, [resolvedTheme, terminalPrefsEpoch]);

  const safeWrite = useCallback((term: XTerm, chunk: string, shouldScroll: boolean = true) => {
    if (!chunk || xtermRef.current !== term) return;
    recordTerminalOutputBytes(chunk.length);
    try {
      term.write(chunk, () => {
        // Callback executado após o write completar
        if (shouldScroll && isAtBottomRef.current) {
          // Se o usuário está no final, faz scroll automático
          term.scrollToBottom();
        } else if (shouldScroll && !isAtBottomRef.current) {
          // Se o usuário não está no final, indica que há novo conteúdo
          setHasNewOutput(true);
        }
      });
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

  const handleStartOrRestartAgent = useCallback((restart: boolean = false) => {
    const api = window.desktopAPI?.terminal;
    const term = xtermRef.current;
    if (!api?.getOrCreateForPane || !paneId || !term) return;
    if (restart) term.reset();
    void api
      .getOrCreateForPane(paneId, {
        cwd,
        command,
        args: stableArgs,
        cols: term.cols,
        rows: term.rows,
        restart,
      })
      .then(async (result) => {
        if (xtermRef.current !== term) return;
        applyPaneAttachResult(result, term);
        await hydrateTerminalBacklog(term, result.ptyId);
      });
  }, [applyPaneAttachResult, hydrateTerminalBacklog, paneId, cwd, command, stableArgs]);

  const handleRestartAgent = useCallback(() => {
    handleStartOrRestartAgent(true);
  }, [handleStartOrRestartAgent]);

  const handleStartAgent = useCallback(() => {
    handleStartOrRestartAgent(false);
  }, [handleStartOrRestartAgent]);

  useEffect(() => {
    if (onStartRequest && !autoStart) {
      onStartRequest(handleStartAgent);
    }
  }, [onStartRequest, autoStart, handleStartAgent]);

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

  const processImageAndInject = useCallback(async (imageBlob: Blob) => {
    const api = window.desktopAPI?.terminal;
    const ptyId = ptyIdRef.current;
    if (!api?.saveTempImage || !api?.write || !ptyId) return;

    try {
      // Convert blob to array buffer
      const arrayBuffer = await imageBlob.arrayBuffer();
      const uint8Array = new Uint8Array(arrayBuffer);
      const imageData = Array.from(uint8Array);

      // Get file extension from MIME type
      const mimeType = imageBlob.type || 'image/png';
      const extension = mimeType.split('/')[1] || 'png';

      // Save image temporarily
      const result = await api.saveTempImage(imageData, extension);

      if (result?.path) {
        // Inject the path into the terminal
        await api.write(ptyId, ` ${result.path}`);
      }
    } catch (error) {
      console.error('[EmbeddedTerminal] Failed to process image:', error);
    }
  }, []);

  const handlePaste = useCallback((e: ClipboardEvent) => {
    const items = e.clipboardData?.items;
    if (!items) return;

    for (const item of items) {
      if (item.type.startsWith('image/')) {
        e.preventDefault();
        const blob = item.getAsFile();
        if (blob) {
          void processImageAndInject(blob);
        }
        break;
      }
    }
  }, [processImageAndInject]);

  const handleAttachImage = useCallback(async () => {
    // Create file input dynamically
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = 'image/*';
    input.onchange = async (e) => {
      const file = (e.target as HTMLInputElement).files?.[0];
      if (file && file.type.startsWith('image/')) {
        await processImageAndInject(file);
      }
    };
    input.click();
  }, [processImageAndInject]);

  const scrollToBottom = useCallback(() => {
    const term = xtermRef.current;
    if (term) {
      term.scrollToBottom();
      isAtBottomRef.current = true;
      setHasNewOutput(false);
    }
  }, []);

  const checkIfAtBottom = useCallback((term: XTerm) => {
    // Verifica se o viewport está no final do buffer
    // baseViewportY é 0-indexed, então comparamos com buffer.length - rows
    const buffer = term.buffer.active;
    const viewportY = term.buffer.active.viewportY;
    const rows = term.rows;
    const bufferLength = buffer.length;

    // Considera "no final" se estiver nas últimas 2 linhas
    const atBottom = viewportY >= bufferLength - rows - 2;
    const wasAtBottom = isAtBottomRef.current;

    isAtBottomRef.current = atBottom;

    // Se voltou para o final, limpa o indicador de novo conteúdo
    if (atBottom && !wasAtBottom) {
      setHasNewOutput(false);
    }
  }, []);

  useEffect(() => {
    const api = window.desktopAPI?.terminal;
    if (!api || !containerRef.current) return;
    let disposed = false;
    let resizeRaf: number | null = null;

    const initialPrefs = loadTerminalAppearance();
    const term = new XTerm({
      cursorBlink: true,
      scrollOnUserInput: true,
      fastScrollModifier: "alt",
      fastScrollSensitivity: 5,
      scrollSensitivity: 1,
      fontSize: initialPrefs.fontSize,
      fontFamily: initialPrefs.fontFamily,
      theme: getXtermColorTheme(resolvedTheme, initialPrefs.useAppThemeColors),
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

    let unlistenOutput: UnlistenFn | null = null;

    const unsubExit = api.onExit((id: string, code: number) => {
      if (id === ptyIdRef.current) {
        ptyIdRef.current = null;
        setExited(code);
        // Só atualiza se realmente mudou (evita re-render)
        if (isWaitingRef.current) {
          isWaitingRef.current = false;
          setIsWaiting(false);
        }
        if (paneId && command) setAgentCanRestart(true);
        onExitRef.current?.(code);
      }
    });

    const unsubAttention = api.onAttention?.((payload: TerminalAttentionPayload) => {
      const id = payload.ptyId ?? payload.paneId;
      const phase = resolveAttentionPhase(payload);
      if (id === ptyIdRef.current) {
        const next = phase === "needs_input";
        if (isWaitingRef.current !== next) {
          isWaitingRef.current = next;
          setIsWaiting(next);
        }
      }
    });

    term.onData((data: string) => {
      const id = ptyIdRef.current;
      if (id) {
        api.write(id, data);
        // Só atualiza quando estava em "waiting" — evita re-render a cada tecla
        if (isWaitingRef.current) {
          isWaitingRef.current = false;
          setIsWaiting(false);
        }
      }
    });

    // Usa evento nativo do xterm para detectar scroll (muito mais eficiente que polling)
    term.onScroll(() => {
      if (!disposed && xtermRef.current === term) {
        checkIfAtBottom(term);
      }
    });

    // Add paste listener for images
    const container = containerRef.current;
    if (container) {
      container.addEventListener('paste', handlePaste as EventListener);
    }

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
          // Se !autoStart, primeiro verifica se já existe sessão
          if (!autoStart && api.getPaneSession) {
            const session = await api.getPaneSession(paneId);
            if (!session || typeof session !== "object" || !("ptyId" in session)) {
              // Sem sessão ativa, notifica parent e espera user iniciar
              onReadyNotStarted?.();
              return;
            }
            // Sessão existe, faz reattach normal
            const result = await api.getOrCreateForPane(paneId, spawnOptions);
            if (disposed || xtermRef.current !== term) return;
            applyPaneAttachResult(result, term);
            // CRÍTICO: Hidrata backlog ANTES de registrar listener (evita race condition)
            await hydrateTerminalBacklog(term, result.ptyId);
          } else {
            // autoStart=true, comportamento normal
            const result = await api.getOrCreateForPane(paneId, spawnOptions);
            if (disposed || xtermRef.current !== term) return;
            applyPaneAttachResult(result, term);
            // CRÍTICO: Hidrata backlog ANTES de registrar listener (evita race condition)
            await hydrateTerminalBacklog(term, result.ptyId);
          }
        } else {
          // Para terminais sem paneId, sempre spawna se autoStart=true
          if (!autoStart) {
            onReadyNotStarted?.();
            return;
          }
          const result = await api.spawn(spawnOptions);
          if (disposed || xtermRef.current !== term) return;
          if (result.error) {
            setError(result.error);
            return;
          }
          if (result.ptyId) ptyIdRef.current = result.ptyId;
          // CRÍTICO: Hidrata backlog ANTES de registrar listener (evita race condition)
          await hydrateTerminalBacklog(term, result.ptyId);
        }

        // AGORA sim, registra listener de output (após backlog estar carregado)
        unlistenOutput = await listen("terminal-output", (event: { payload?: Record<string, unknown> }) => {
          if (disposed || xtermRef.current !== term) return;
          const payload = event?.payload ?? {};
          const id = typeof payload.ptyId === "string" ? payload.ptyId : "";
          const data = typeof payload.data === "string" ? payload.data : "";
          if (id !== ptyIdRef.current) return;
          safeWrite(term, data);
        });
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        // Garante cleanup de listener mesmo em caso de erro
        if (disposed && unlistenOutput) {
          void unlistenOutput();
          unlistenOutput = null;
        }
      }
    };

    void mountSession();

    let isResizing = false;
    const handleResize = () => {
      if (disposed || xtermRef.current !== term || isResizing) return;
      if (fitAddonRef.current && ptyIdRef.current) {
        // Proteção anti-loop: marca que estamos fazendo resize
        isResizing = true;
        try {
          safeFit();
          const { cols, rows } = term;
          if (cols && rows) {
            api.resize(ptyIdRef.current, cols, rows);
          }
        } finally {
          // Libera flag após pequeno delay para evitar re-trigger imediato
          setTimeout(() => {
            isResizing = false;
          }, 50);
        }
      }
    };

    let lastObservedW = 0;
    let lastObservedH = 0;
    const resizeObserver = new ResizeObserver((entries) => {
      // Ignora eventos durante resize ativo (proteção anti-loop)
      if (isResizing) return;

      const cr = entries[0]?.contentRect;
      if (cr) {
        const w = Math.round(cr.width);
        const h = Math.round(cr.height);
        // Ignora se dimensões não mudaram (proteção adicional)
        if (w === lastObservedW && h === lastObservedH) return;
        lastObservedW = w;
        lastObservedH = h;
      }
      if (resizeRaf !== null) cancelAnimationFrame(resizeRaf);
      resizeRaf = requestAnimationFrame(handleResize);
    });
    resizeObserver.observe(containerRef.current);

    return () => {
      disposed = true;
      if (resizeRaf !== null) cancelAnimationFrame(resizeRaf);
      resizeObserver.disconnect();
      void unlistenOutput?.();
      unsubExit();
      unsubAttention?.();
      // Cleanup paste listener
      if (container) {
        container.removeEventListener('paste', handlePaste as EventListener);
      }
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
    autoStart,
    checkIfAtBottom,
    command,
    cwd,
    handlePaste,
    hydrateTerminalBacklog,
    onReadyNotStarted,
    paneId,
    safeWrite,
    shellCommand,
    stableArgs,
  ]);

  return (
    <div className={`flex h-full min-h-0 min-w-0 flex-col overflow-hidden rounded-lg border transition-[border-color,box-shadow] duration-200 ${
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
          {command && (
            <Button
              variant="ghost"
              size="icon"
              className="h-7 w-7"
              onClick={handleAttachImage}
              aria-label="Anexar imagem"
              title="Anexar imagem (ou use Cmd+V)"
            >
              <Paperclip className="h-4 w-4" />
            </Button>
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
      <div className="relative min-h-0 flex-1 overflow-hidden">
        <div
          ref={containerRef}
          className="h-full w-full overflow-hidden bg-background p-1"
        />
        {hasNewOutput && (
          <div className="absolute bottom-4 right-4 flex items-center gap-2">
            <Button
              onClick={scrollToBottom}
              size="sm"
              className="shadow-lg"
              variant="default"
            >
              <ArrowDown className="mr-2 h-4 w-4" />
              Novo conteúdo
            </Button>
          </div>
        )}
      </div>
    </div>
  );
}
