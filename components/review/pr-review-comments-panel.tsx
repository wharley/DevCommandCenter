"use client";

import React, { useCallback, useEffect, useState } from "react";
import {
  ChevronDown,
  ExternalLink,
  Loader2,
  MessageSquare,
  RefreshCw,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { cn } from "@/lib/utils";
import { toast } from "sonner";

export type ForgePrReviewComment = {
  path: string;
  line: number | null;
  side: string;
  body: string;
  author: string;
  url: string;
  createdAt: string;
};

type FetchState =
  | { status: "idle" }
  | { status: "loading" }
  | { status: "skipped"; reason?: string }
  | { status: "error"; message: string }
  | { status: "ok"; forge?: string; comments: ForgePrReviewComment[] };

export type PrReviewCommentsPanelProps = {
  combId: string | null | undefined;
  /** Só o target principal da Missão (worktree) tem PR ligado ao comb. */
  enabled: boolean;
  activeFilePath: string | null;
  idKey: string;
  compact?: boolean;
};

function parseComments(raw: unknown): ForgePrReviewComment[] {
  if (!Array.isArray(raw)) return [];
  const out: ForgePrReviewComment[] = [];
  for (const c of raw) {
    if (!c || typeof c !== "object") continue;
    const o = c as Record<string, unknown>;
    out.push({
      path: typeof o.path === "string" ? o.path : "",
      line: typeof o.line === "number" ? o.line : null,
      side: typeof o.side === "string" ? o.side : "RIGHT",
      body: typeof o.body === "string" ? o.body : "",
      author: typeof o.author === "string" ? o.author : "",
      url: typeof o.url === "string" ? o.url : "",
      createdAt: typeof o.createdAt === "string" ? o.createdAt : "",
    });
  }
  return out;
}

export function PrReviewCommentsPanel({
  combId,
  enabled,
  activeFilePath,
  idKey,
  compact,
}: PrReviewCommentsPanelProps) {
  const [open, setOpen] = useState(true);
  const [state, setState] = useState<FetchState>({ status: "idle" });

  const load = useCallback(async () => {
    if (!combId?.trim() || !enabled) return;
    const api = window.desktopAPI?.forge?.fetchPrReviewComments;
    if (!api) {
      setState({ status: "error", message: "Indisponível neste ambiente." });
      return;
    }
    setState({ status: "loading" });
    try {
      const res = await api(combId, undefined);
      const r = res as {
        skipped?: boolean;
        reason?: string;
        success?: boolean;
        error?: string;
        forge?: string;
        comments?: unknown;
      };
      if (r.skipped) {
        setState({ status: "skipped", reason: r.reason });
        return;
      }
      if (r.success === false && r.error) {
        setState({ status: "error", message: r.error });
        return;
      }
      const comments = parseComments(r.comments);
      setState({
        status: "ok",
        forge: typeof res?.forge === "string" ? res.forge : undefined,
        comments,
      });
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Erro ao carregar comentários";
      setState({ status: "error", message: msg });
      toast.error(msg);
    }
  }, [combId, enabled]);

  useEffect(() => {
    if (!enabled || !combId?.trim()) {
      setState({ status: "idle" });
      return;
    }
    void load();
  }, [combId, enabled, load]);

  if (!enabled || !combId?.trim()) {
    return null;
  }

  return (
    <Collapsible
      open={open}
      onOpenChange={setOpen}
      className="rounded-md border border-border bg-muted/10"
    >
      <CollapsibleTrigger className="flex w-full items-center justify-between gap-2 rounded-t-md px-3 py-2 text-left text-xs font-medium hover:bg-muted/40">
        <span className="flex items-center gap-2 min-w-0">
          <MessageSquare className="h-3.5 w-3.5 shrink-0 opacity-80" />
          <span className="truncate">Comentários inline do PR</span>
          {state.status === "ok" ? (
            <span className="shrink-0 rounded border border-border/80 bg-background/60 px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground">
              {state.comments.length}
            </span>
          ) : null}
        </span>
        <span className="flex items-center gap-1 shrink-0">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-7 px-2"
            disabled={state.status === "loading"}
            onClick={(e) => {
              e.stopPropagation();
              void load();
            }}
            title="Atualizar comentários"
          >
            {state.status === "loading" ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <RefreshCw className="h-3.5 w-3.5" />
            )}
          </Button>
          <ChevronDown
            className={cn(
              "h-4 w-4 opacity-70 transition-transform",
              open && "rotate-180",
            )}
          />
        </span>
      </CollapsibleTrigger>
      <CollapsibleContent className="border-t border-border/60 px-3 pb-3 pt-0 text-xs">
        <div className="space-y-2 pt-2">
          {state.status === "loading" ? (
            <p className="flex items-center gap-2 text-muted-foreground">
              <Loader2 className="h-3.5 w-3.5 animate-spin" /> A carregar…
            </p>
          ) : null}
          {state.status === "skipped" ? (
            <p className="text-[11px] leading-relaxed text-muted-foreground">
              {state.reason === "no_forge_link"
                ? "Sem PR/MR ligado a este workspace. Abre um PR/MR na branch ou usa a sincronização de ligação na lista de workspaces."
                : "Não foi possível resolver comentários (sem ligação ou remoto)."}
            </p>
          ) : null}
          {state.status === "error" ? (
            <p className="text-[11px] text-destructive">{state.message}</p>
          ) : null}
          {state.status === "ok" && state.comments.length === 0 ? (
            <p className="text-[11px] text-muted-foreground">
              Nenhum comentário inline no{" "}
              {state.forge === "gitlab" ? "MR" : "PR"} (ou ainda não há revisões
              posicionadas no diff).
            </p>
          ) : null}
          {state.status === "ok" && state.comments.length > 0 ? (
            <ul
              className={cn(
                "max-h-[min(240px,40vh)] space-y-2 overflow-y-auto pr-1",
                compact && "max-h-[min(180px,32vh)]",
              )}
            >
              {state.comments.map((c, i) => (
                <li
                  key={`${idKey}-all-${c.path}-${c.line}-${i}`}
                  className={cn(
                    "rounded border p-2 transition-colors",
                    activeFilePath && c.path === activeFilePath
                      ? "border-primary/35 bg-primary/8"
                      : "border-border/60 bg-muted/20",
                  )}
                >
                  <div className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
                    <span className="font-mono text-[10px] text-muted-foreground break-all">
                      {c.path}
                    </span>
                    <span className="font-mono text-[10px] text-muted-foreground">
                      L{c.line ?? "?"}
                    </span>
                    <span className="text-[10px] text-foreground/80">{c.author}</span>
                    {c.url ? (
                      <a
                        href={c.url}
                        target="_blank"
                        rel="noreferrer"
                        className="ml-auto inline-flex shrink-0 text-primary hover:underline"
                      >
                        <ExternalLink className="h-3 w-3" />
                      </a>
                    ) : null}
                  </div>
                  <p className="mt-1 whitespace-pre-wrap text-[11px] text-muted-foreground">
                    {c.body}
                  </p>
                </li>
              ))}
            </ul>
          ) : null}
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}
