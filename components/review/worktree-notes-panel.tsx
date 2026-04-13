"use client";

import React, { useCallback, useEffect, useRef, useState } from "react";
import { Check, ChevronDown, FileText, Loader2 } from "lucide-react";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";
import { toast } from "sonner";

const DEBOUNCE_MS = 1500;
const MAX_NOTE_BYTES = 512 * 1024;

type SaveState =
  | "idle"
  | "loading"
  | "dirty"
  | "saving"
  | "saved"
  | "error";

export type WorktreeNotesPanelProps = {
  worktreePath: string;
  /** Chave estável para ids de acessibilidade (ex. storageKey da revisão). */
  idKey: string;
  compact?: boolean;
};

export function WorktreeNotesPanel({
  worktreePath,
  idKey,
  compact,
}: WorktreeNotesPanelProps) {
  const [open, setOpen] = useState(true);
  const [text, setText] = useState("");
  const [loaded, setLoaded] = useState(false);
  const [saveState, setSaveState] = useState<SaveState>("idle");
  const lastSavedRef = useRef("");
  const textRef = useRef("");
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  textRef.current = text;

  const api =
    typeof window !== "undefined" ? window.desktopAPI?.worktree : undefined;

  const load = useCallback(async () => {
    if (!worktreePath?.trim()) {
      setText("");
      lastSavedRef.current = "";
      setLoaded(true);
      return;
    }
    if (!api?.readNotes) {
      setLoaded(true);
      return;
    }
    setSaveState("loading");
    try {
      const res = await api.readNotes(worktreePath);
      if (res?.success === true && typeof res.content === "string") {
        setText(res.content);
        lastSavedRef.current = res.content;
      } else {
        setText("");
        lastSavedRef.current = "";
        if (res?.success === false && res.error) {
          toast.error(res.error);
        }
      }
    } catch {
      toast.error("Não foi possível carregar as notas.");
    } finally {
      setLoaded(true);
      setSaveState("idle");
    }
  }, [worktreePath, api]);

  useEffect(() => {
    setLoaded(false);
    void load();
  }, [load]);

  const flushSave = useCallback(async () => {
    if (!api?.writeNotes || !worktreePath?.trim()) return;
    const next = textRef.current;
    if (next === lastSavedRef.current) return;
    const byteLen = new TextEncoder().encode(next).length;
    if (byteLen > MAX_NOTE_BYTES) {
      setSaveState("error");
      toast.error(
        `Notas excedem o máximo de ${MAX_NOTE_BYTES / 1024} KiB (UTF-8).`,
      );
      return;
    }
    setSaveState("saving");
    try {
      const res = await api.writeNotes(worktreePath, next);
      if (res?.success) {
        lastSavedRef.current = next;
        setSaveState("saved");
        window.setTimeout(() => {
          setSaveState((s) => (s === "saved" ? "idle" : s));
        }, 2000);
      } else {
        setSaveState("error");
        toast.error(res?.error ?? "Erro ao guardar notas");
      }
    } catch {
      setSaveState("error");
      toast.error("Erro ao guardar notas");
    }
  }, [api, worktreePath]);

  useEffect(() => {
    if (!loaded || !api?.writeNotes) return;
    if (text === lastSavedRef.current) return;
    setSaveState("dirty");
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => {
      debounceRef.current = null;
      void flushSave();
    }, DEBOUNCE_MS);
    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, [text, loaded, api, flushSave]);

  useEffect(() => {
    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, []);

  if (!worktreePath?.trim()) {
    return null;
  }

  if (!api?.readNotes || !api?.writeNotes) {
    return (
      <div className="rounded-md border border-border/60 bg-muted/10 px-3 py-2 text-[11px] text-muted-foreground">
        Notas por worktree disponíveis no ambiente desktop (Tauri).
      </div>
    );
  }

  const fieldId = `worktree-notes-${idKey.replace(/[^a-zA-Z0-9_-]/g, "-")}`;

  return (
    <Collapsible
      open={open}
      onOpenChange={setOpen}
      className="rounded-md border border-border bg-muted/10"
    >
      <CollapsibleTrigger className="flex w-full items-center justify-between gap-2 rounded-t-md px-3 py-2 text-left text-xs font-medium hover:bg-muted/40">
        <span className="flex items-center gap-2">
          <FileText className="h-3.5 w-3.5 shrink-0 opacity-80" />
          Notas do workspace
          <span className="font-normal text-[10px] text-muted-foreground">
            (.dcc/notes.md)
          </span>
        </span>
        <span className="flex items-center gap-2 shrink-0">
          {saveState === "loading" ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin text-muted-foreground" />
          ) : saveState === "saving" ? (
            <span className="text-[10px] text-muted-foreground">A guardar…</span>
          ) : saveState === "saved" ? (
            <span className="flex items-center gap-0.5 text-[10px] text-emerald-600 dark:text-emerald-400">
              <Check className="h-3 w-3" /> Guardado
            </span>
          ) : saveState === "dirty" ? (
            <span className="text-[10px] text-muted-foreground">Alterações…</span>
          ) : null}
          <ChevronDown
            className={cn(
              "h-4 w-4 opacity-70 transition-transform",
              open && "rotate-180",
            )}
          />
        </span>
      </CollapsibleTrigger>
      <CollapsibleContent className="border-t border-border/60 px-3 pb-3 pt-0">
        <div className="space-y-1.5 pt-2">
          <Label
            htmlFor={fieldId}
            className="text-[10px] uppercase text-muted-foreground"
          >
            Markdown livre — contexto humano para esta Missão
          </Label>
          <Textarea
            id={fieldId}
            value={text}
            onChange={(e) => setText(e.target.value)}
            disabled={!loaded || saveState === "loading"}
            placeholder="Objetivos, decisões, links, lembretes… Fica em `.dcc/notes.md` no repositório da worktree (podes commitar)."
            className={cn(
              "min-h-[100px] resize-y font-mono text-xs leading-relaxed",
              compact && "min-h-[72px]",
            )}
            spellCheck={false}
          />
          <p className="text-[10px] text-muted-foreground">
            Máx. ~{MAX_NOTE_BYTES / 1024} KiB (UTF-8). Guarda automática após
            escrever.
          </p>
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}
