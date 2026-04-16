"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Terminal as XTerm } from "xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { SearchAddon } from "@xterm/addon-search";
import "xterm/css/xterm.css";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import {
  X,
  Paperclip,
  ArrowDown,
  ChevronUp,
  ChevronDown,
  Signal,
  AlertTriangle,
} from "lucide-react";
import { toast } from "sonner";
import { useTheme } from "@/components/theme-provider";
import {
  type TerminalAttentionPayload,
  resolveAttentionPhase,
} from "@/lib/terminal/attention-types";
import {
  createTerminalOutputMetricsTracker,
  recordTerminalOutputBytes,
} from "@/lib/terminal/output-metrics";
import { getTerminalAppearancePreferences } from "@/lib/terminal/terminal-preferences";
import { getXtermColorTheme } from "@/lib/terminal/xterm-theme";
import { writePtyInputInChunks } from "@/lib/terminal/pty-input-write";
import { handleOsc52Payload } from "@/lib/terminal/osc52";
import { showNativeNotification } from "@/lib/notifications";

export interface EmbeddedTerminalProps {
  cwd: string;
  command?: string;
  args?: string[];
  onClose?: () => void;
  onExit?: (code: number) => void;
  /** Chamado quando um PTY fica associado ao pane (spawn ou reattach). Útil para badges no pai. */
  onSessionActive?: () => void;
  /** Optional label for the terminal */
  title?: string;
  /**
   * When set, PTY is keyed by paneId (Workspace architecture): getOrCreateForPane is used
   * and the PTY is NOT killed on unmount for reattach.
   */
  paneId?: string;
  /** ID do comb (worktree) para metadata de notificações e navegação */
  combId?: string;
  /** ID do projeto para metadata de notificações e navegação */
  projectId?: string;
}

type PaneAttachResult = {
  ptyId?: string;
  error?: string;
  session?: any | null;
};

function formatTerminalThroughput(bytesPerSecond: number): string {
  if (bytesPerSecond >= 1024 * 1024) {
    return `${(bytesPerSecond / (1024 * 1024)).toFixed(1)} MiB/s`;
  }
  return `${(bytesPerSecond / 1024).toFixed(0)} KiB/s`;
}

export function EmbeddedTerminal({
  cwd,
  command,
  args = [],
  onClose,
  onExit,
  onSessionActive,
  title,
  paneId,
  combId,
  projectId,
}: EmbeddedTerminalProps) {
  const { resolvedTheme } = useTheme();
  const [terminalPrefsEpoch, setTerminalPrefsEpoch] = useState(0);
  const containerRef = useRef<HTMLDivElement>(null);
  const xtermRef = useRef<XTerm | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const searchAddonRef = useRef<SearchAddon | null>(null);
  const [outputHealthTracker] = useState(() => createTerminalOutputMetricsTracker());
  const ptyIdRef = useRef<string | null>(null);
  /** Serializa escritas no PTY (teclado + paste) e evita promise rejeitada sem catch. */
  const ptyWriteChainRef = useRef<Promise<void>>(Promise.resolve());
  const onExitRef = useRef<typeof onExit>(onExit);
  /** Rastreia se usuário está no final do terminal para auto-scroll inteligente */
  const isAtBottomRef = useRef(true);
  /** Cooldown para bell notifications (evita spam) - timestamp da última notificação */
  const lastBellNotificationRef = useRef<number>(0);
  /** Rastreia estado de waiting para evitar re-renders desnecessários */
  const isWaitingRef = useRef(false);
  /** Contador de output novo quando usuário não está no final */
  const [hasNewOutput, setHasNewOutput] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [exited, setExited] = useState<number | null>(null);
  const [isWaiting, setIsWaiting] = useState(false);
  /** Pane agent: mostrar reinício quando o processo terminou ou só há sessão encerrada no main. */
  const [agentCanRestart, setAgentCanRestart] = useState(false);
  const [outputHealth, setOutputHealth] = useState(() => outputHealthTracker.getSnapshot());
  /** Search bar visibility and state */
  const [searchVisible, setSearchVisible] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  /** Espelha sessão ativa para mostrar ações que dependem de `ptyId` (ex.: envio de sinais). */
  const [ptySessionId, setPtySessionId] = useState<string | null>(null);
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
    return outputHealthTracker.subscribe(setOutputHealth);
  }, [outputHealthTracker]);

  useEffect(() => () => outputHealthTracker.dispose(), [outputHealthTracker]);

  useEffect(() => {
    const onAction = (ev: Event) => {
      const ce = ev as CustomEvent<{ type?: string }>;
      if (ce.detail?.type !== "clearScrollback") return;
      const t = xtermRef.current;
      if (t) t.clear();
      if (paneId && window.desktopAPI?.terminal?.clearPersistedScrollback) {
        void window.desktopAPI.terminal.clearPersistedScrollback(paneId);
      }
    };
    window.addEventListener("dcc-terminal-action", onAction as EventListener);
    return () => window.removeEventListener("dcc-terminal-action", onAction as EventListener);
  }, [paneId]);

  // Search keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === "f") {
        e.preventDefault();
        setSearchVisible((prev) => !prev);
      }
      if (e.key === "Escape" && searchVisible) {
        setSearchVisible(false);
        setSearchQuery("");
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [searchVisible]);

  useEffect(() => {
    const term = xtermRef.current;
    if (!term) return;
    const prefs = getTerminalAppearancePreferences();
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
    const finishTracking = outputHealthTracker.beginChunk(chunk.length);
    try {
      term.write(chunk, () => {
        finishTracking?.();
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
      finishTracking?.();
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
        setPtySessionId(result.ptyId);
        setAgentCanRestart(false);
        setExited(null);
        onSessionActive?.();
      } else {
        setPtySessionId(null);
        if (result.session?.status === "exited" && command) {
          setAgentCanRestart(true);
        }
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
    [command, onSessionActive, safeWrite],
  );

  const handleSendSignal = useCallback(async (sig: "SIGINT" | "SIGTERM" | "SIGKILL") => {
    const id = ptyIdRef.current;
    const send = window.desktopAPI?.terminal?.sendSignal;
    if (!id || !send) return;
    try {
      const r = await send(id, sig);
      if (!r?.ok) {
        toast.error(r?.error ?? "Falha ao enviar sinal");
      }
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : "Falha ao enviar sinal");
    }
  }, []);

  const enqueuePtyUserInput = useCallback((data: string) => {
    if (!data) return;
    const api = window.desktopAPI?.terminal;
    if (!api?.write) return;

    ptyWriteChainRef.current = ptyWriteChainRef.current
      .catch(() => {
        /* falha anterior: permite continuar a fila */
      })
      .then(async () => {
        const id = ptyIdRef.current;
        if (!id) return;
        await writePtyInputInChunks(api.write.bind(api), id, data);
        if (isWaitingRef.current) {
          isWaitingRef.current = false;
          setIsWaiting(false);
        }
      })
      .catch((e: unknown) => {
        if (!xtermRef.current) return;
        const msg = e instanceof Error ? e.message : String(e);
        setError(msg);
      });
  }, []);

  const handleStartOrRestartAgent = useCallback((restart: boolean = false) => {
    const api = window.desktopAPI?.terminal;
    const term = xtermRef.current;
    if (!api?.getOrCreateForPane || !paneId || !term) return;
    try {
      fitAddonRef.current?.fit();
    } catch {
      /* ignore */
    }
    const cols = Math.max(term.cols, 2) || 80;
    const rows = Math.max(term.rows, 2) || 24;
    if (restart) term.reset();
    void api
      .getOrCreateForPane(paneId, {
        cwd,
        command,
        args: stableArgs,
        cols,
        rows,
        restart,
      })
      .then(async (result) => {
        if (xtermRef.current !== term) return;
        applyPaneAttachResult(result, term);
        await hydrateTerminalBacklog(term, result.ptyId);
      })
      .catch((e: unknown) => {
        setError(e instanceof Error ? e.message : String(e));
      });
  }, [applyPaneAttachResult, hydrateTerminalBacklog, paneId, cwd, command, stableArgs]);

  const handleRestartAgent = useCallback(() => {
    handleStartOrRestartAgent(true);
  }, [handleStartOrRestartAgent]);

  const killPty = useCallback(() => {
    const id = ptyIdRef.current;
    if (id && window.desktopAPI?.terminal?.kill) {
      window.desktopAPI.terminal.kill(id);
      ptyIdRef.current = null;
      setPtySessionId(null);
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
        ptyWriteChainRef.current = ptyWriteChainRef.current
          .catch(() => {})
          .then(async () => {
            const id = ptyIdRef.current;
            if (!id) return;
            await writePtyInputInChunks(api.write.bind(api), id, ` ${result.path}`);
          })
          .catch((err: unknown) => {
            console.error("[EmbeddedTerminal] Failed to inject image path:", err);
          });
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

  const performSearch = useCallback((query: string, direction: "next" | "prev" = "next") => {
    if (!searchAddonRef.current || !query) return;

    const found = direction === "next"
      ? searchAddonRef.current.findNext(query, { caseSensitive: false })
      : searchAddonRef.current.findPrevious(query, { caseSensitive: false });

    return found;
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

    const initialPrefs = getTerminalAppearancePreferences();
    const term = new XTerm({
      cursorBlink: initialPrefs.cursorBlink,
      cursorStyle: initialPrefs.cursorStyle,
      scrollback: initialPrefs.scrollback,
      scrollOnUserInput: true,
      fastScrollModifier: "alt",
      fastScrollSensitivity: initialPrefs.fastScrollSensitivity,
      scrollSensitivity: 1,
      rightClickSelectsWord: initialPrefs.rightClickSelectsWord,
      fontSize: initialPrefs.fontSize,
      fontFamily: initialPrefs.fontFamily,
      theme: getXtermColorTheme(resolvedTheme, initialPrefs.useAppThemeColors),
      windowOptions: {
        // Desabilita queries automáticas de cores que causam caracteres estranhos
        // quando o terminal ganha foco. OSC 52 (clipboard) continua funcionando
        // pois é registrado manualmente via registerOscHandler
        getWinSizePixels: false,
        getCellSizePixels: false,
        setWinPosition: false,
        setWinSizePixels: false,
        raiseWin: false,
        lowerWin: false,
        refreshWin: false,
        setWinLines: false,
        maximizeWin: false,
        fullscreenWin: false,
        restoreWin: false,
      },
    });

    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);

    // WebGL Addon for better performance
    try {
      const webglAddon = new WebglAddon();
      webglAddon.onContextLoss(() => {
        // Fallback to canvas renderer
        webglAddon.dispose();
      });
      term.loadAddon(webglAddon);
    } catch (e) {
      console.warn("WebGL addon not available, using canvas renderer:", e);
    }

    // Search Addon
    const searchAddon = new SearchAddon();
    term.loadAddon(searchAddon);
    searchAddonRef.current = searchAddon;

    term.open(containerRef.current);
    xtermRef.current = term;
    fitAddonRef.current = fitAddon;

    // Dá foco automático ao terminal para evitar necessidade de duplo clique
    term.focus();

    const osc52Dispose = term.parser.registerOscHandler(52, (data) =>
      handleOsc52Payload(data, {
        writeText: (text) => navigator.clipboard.writeText(text),
        sendToPty: enqueuePtyUserInput,
      }),
    );
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
        setPtySessionId(null);
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
      enqueuePtyUserInput(data);
    });

    // Copy on select
    if (initialPrefs.copyOnSelect) {
      term.onSelectionChange(() => {
        const selection = term.getSelection();
        if (selection) {
          navigator.clipboard.writeText(selection).catch(() => {
            // Ignore clipboard errors
          });
        }
      });
    }

    // Bell handler with cooldown to prevent spam
    term.onBell(() => {
      const prefs = getTerminalAppearancePreferences();

      if (prefs.bellStyle === "none") return;

      // Visual bell (sempre executado, sem cooldown)
      if (prefs.bellStyle === "visual" || prefs.bellStyle === "both") {
        const container = containerRef.current;
        if (container) {
          container.style.animation = "terminal-bell-flash 0.3s ease-in-out";
          setTimeout(() => {
            if (container) container.style.animation = "";
          }, 300);
        }
      }

      // Sound bell via native OS notification (respects user notification preferences)
      if (prefs.bellStyle === "sound" || prefs.bellStyle === "both") {
        const now = Date.now();
        const BELL_COOLDOWN_MS = 3000; // 3 segundos de cooldown entre notificações

        // Aplica cooldown para evitar spam de notificações
        if (now - lastBellNotificationRef.current < BELL_COOLDOWN_MS) {
          return;
        }

        lastBellNotificationRef.current = now;

        const terminalLabel = title ?? "Terminal";
        const displayPath = cwd.length > 50 ? `...${cwd.slice(-47)}` : cwd;

        // Ações disponíveis quando há metadata completa para navegação
        const hasNavigationContext = paneId && combId && projectId;
        const actions = hasNavigationContext
          ? [
              { id: "reply", label: "Abrir painel" },
              { id: "dismiss", label: "Dispensar" },
            ]
          : [{ id: "dismiss", label: "Dispensar" }];

        void showNativeNotification({
          title: `🔔 ${terminalLabel}`,
          body: `Terminal activity detected in:\n${displayPath}`,
          icon: "auto",
          sound: true,
          source: "terminal-bell",
          paneId: paneId ?? undefined,
          combId: combId ?? undefined,
          projectId: projectId ?? undefined,
          notificationId: paneId ? `bell-${paneId}` : `bell-${ptyIdRef.current}`,
          actions,
        }).catch((err: unknown) => {
          console.warn("[EmbeddedTerminal] Failed to show bell notification:", err);
        });
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

    const spawnCols = Math.max(term.cols, 2) || 80;
    const spawnRows = Math.max(term.rows, 2) || 24;
    const spawnOptions =
      command && command.trim().length > 0
        ? {
            cwd,
            command: shellCommand,
            args: ["-il", "-c", `${command} ${stableArgs.join(" ")}`.trim()],
            cols: spawnCols,
            rows: spawnRows,
          }
        : {
            cwd,
            command: shellCommand,
            args: ["-il"],
            cols: spawnCols,
            rows: spawnRows,
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
          if (result.ptyId) {
            ptyIdRef.current = result.ptyId;
            setPtySessionId(result.ptyId);
          }
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
      osc52Dispose.dispose();
      ptyWriteChainRef.current = Promise.resolve();
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
    checkIfAtBottom,
    command,
    cwd,
    enqueuePtyUserInput,
    handlePaste,
    hydrateTerminalBacklog,
    paneId,
    safeWrite,
    shellCommand,
    stableArgs,
  ]);

  const slowLinkBadgeTitle = outputHealth.isSlowLink
    ? [
        "Ligação lenta detectada",
        outputHealth.reason === "pending-writes"
          ? "motivo: fila pendente"
          : outputHealth.reason === "render-latency"
            ? "motivo: latência de render"
            : outputHealth.reason === "throughput"
              ? "motivo: throughput alto"
              : null,
        `throughput ${formatTerminalThroughput(outputHealth.bytesPerSecond)}`,
        `fila ${outputHealth.pendingWrites}`,
        outputHealth.avgWriteLatencyMs !== null
          ? `latência média ${outputHealth.avgWriteLatencyMs.toFixed(1)} ms`
          : null,
      ]
        .filter(Boolean)
        .join(" · ")
    : "";

  return (
    <div className={`flex h-full min-h-0 min-w-0 flex-col overflow-hidden rounded-lg border transition-[border-color,box-shadow] duration-200 ${
      isWaiting ? "border-primary shadow-[0_0_15px_rgba(var(--primary),0.3)] ring-1 ring-primary" : "border-border"
    } ${outputHealth.isSlowLink ? "shadow-[0_0_0_1px_rgba(245,158,11,0.28)]" : ""} bg-background`}>
      <div className={`flex items-center justify-between border-b px-2 py-1.5 ${
        isWaiting ? "border-primary bg-primary/5" : "border-border"
      }`}>
        <span className="truncate text-xs text-muted-foreground">
          {title ?? "Terminal"} — {cwd}
        </span>
        <div className="flex items-center gap-1">
          {outputHealth.isSlowLink && (
            <Badge
              variant="outline"
              className="border-amber-500/40 bg-amber-500/10 text-amber-700 dark:text-amber-300"
              title={slowLinkBadgeTitle}
            >
              <AlertTriangle className="h-3 w-3" />
              Ligação lenta
              <span className="hidden sm:inline">
                {" "}
                · {formatTerminalThroughput(outputHealth.bytesPerSecond)}
              </span>
            </Badge>
          )}
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
          {ptySessionId &&
            exited === null &&
            typeof window.desktopAPI?.terminal?.sendSignal === "function" && (
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    className="h-7 w-7"
                    title="Enviar sinal ao grupo de processos (SIGINT / SIGTERM / SIGKILL)"
                    aria-label="Sinais do processo"
                  >
                    <Signal className="h-4 w-4" />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" className="w-56">
                  <DropdownMenuItem onClick={() => void handleSendSignal("SIGINT")}>
                    SIGINT — interromper (Ctrl+C)
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={() => void handleSendSignal("SIGTERM")}>
                    SIGTERM — encerramento amigável
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    className="text-destructive focus:text-destructive"
                    onClick={() => void handleSendSignal("SIGKILL")}
                  >
                    SIGKILL — forçar fim
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
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
        {searchVisible && (
          <div className="absolute right-2 top-2 z-10 flex items-center gap-2 rounded-md border bg-background p-2 shadow-lg">
            <Input
              type="text"
              placeholder="Search..."
              value={searchQuery}
              onChange={(e) => {
                setSearchQuery(e.target.value);
                performSearch(e.target.value);
              }}
              className="w-64"
              autoFocus
            />
            <Button
              size="sm"
              variant="ghost"
              onClick={() => performSearch(searchQuery, "prev")}
              aria-label="Previous match"
            >
              <ChevronUp className="h-4 w-4" />
            </Button>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => performSearch(searchQuery, "next")}
              aria-label="Next match"
            >
              <ChevronDown className="h-4 w-4" />
            </Button>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => {
                setSearchVisible(false);
                setSearchQuery("");
              }}
              aria-label="Close search"
            >
              <X className="h-4 w-4" />
            </Button>
          </div>
        )}
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
