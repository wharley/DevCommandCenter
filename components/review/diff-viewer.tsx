"use client";

import React, { useMemo } from "react";
import { cn } from "@/lib/utils";
import { FileCode, X, Plus, Minus } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Badge } from "@/components/ui/badge";

type DiffFile = {
  path: string;
  status: string;
  diff: string;
  insertions?: number;
  deletions?: number;
};

interface DiffViewerProps {
  file: DiffFile;
  onClose: () => void;
}

const STATUS_LABELS: Record<string, string> = {
  A: "Added",
  M: "Modified",
  D: "Deleted",
  R: "Renamed",
};

const STATUS_STYLES: Record<string, { bg: string; text: string }> = {
  A: { bg: "bg-emerald-500/20", text: "text-emerald-600 dark:text-emerald-400" },
  M: { bg: "bg-amber-500/20", text: "text-amber-600 dark:text-amber-400" },
  D: { bg: "bg-rose-500/20", text: "text-rose-600 dark:text-rose-400" },
  R: { bg: "bg-blue-500/20", text: "text-blue-600 dark:text-blue-400" },
};

function fileBasename(path: string) {
  return path.split(/[/\\]/).pop() ?? path;
}

function fileDir(path: string) {
  const parts = path.split(/[/\\]/);
  return parts.length <= 1 ? "" : parts.slice(0, -1).join("/");
}

type ParsedLine =
  | { type: "add"; content: string }
  | { type: "del"; content: string }
  | { type: "hunk"; content: string }
  | { type: "meta"; content: string }
  | { type: "ctx"; content: string };

function parseDiffLines(raw: string): ParsedLine[] {
  return raw.split("\n").map((line): ParsedLine => {
    if (line.startsWith("+") && !line.startsWith("+++")) return { type: "add", content: line };
    if (line.startsWith("-") && !line.startsWith("---")) return { type: "del", content: line };
    if (line.startsWith("@@")) return { type: "hunk", content: line };
    if (
      line.startsWith("diff ") ||
      line.startsWith("index ") ||
      line.startsWith("--- ") ||
      line.startsWith("+++ ")
    )
      return { type: "meta", content: line };
    return { type: "ctx", content: line };
  });
}

export function DiffViewer({ file, onClose }: DiffViewerProps) {
  const statusKey = file.status.charAt(0).toUpperCase();
  const statusStyle = STATUS_STYLES[statusKey] ?? { bg: "bg-muted", text: "text-muted-foreground" };
  const statusLabel = STATUS_LABELS[statusKey] ?? statusKey;
  const dir = fileDir(file.path);
  const basename = fileBasename(file.path);

  const lines = useMemo(() => parseDiffLines(file.diff || ""), [file.diff]);

  const isEmpty = !file.diff?.trim();

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden">
      {/* Header */}
      <div className="flex shrink-0 items-center gap-2 border-b border-border px-3 py-2">
        <FileCode className="h-4 w-4 shrink-0 text-muted-foreground" />
        <div className="min-w-0 flex-1">
          <span className="font-mono text-sm font-medium">{basename}</span>
          {dir && (
            <span className="ml-1.5 text-xs text-muted-foreground">{dir}</span>
          )}
        </div>
        <Badge
          className={cn(
            "shrink-0 rounded px-1.5 py-0.5 text-[10px] font-bold uppercase",
            statusStyle.bg,
            statusStyle.text,
          )}
          variant="outline"
        >
          {statusLabel}
        </Badge>
        {(file.insertions ?? 0) > 0 && (
          <span className="flex items-center gap-0.5 font-mono text-[11px] text-emerald-600 dark:text-emerald-400">
            <Plus className="h-3 w-3" />
            {file.insertions}
          </span>
        )}
        {(file.deletions ?? 0) > 0 && (
          <span className="flex items-center gap-0.5 font-mono text-[11px] text-rose-600 dark:text-rose-400">
            <Minus className="h-3 w-3" />
            {file.deletions}
          </span>
        )}
        <Button
          variant="ghost"
          size="icon"
          className="h-6 w-6 shrink-0"
          onClick={onClose}
          title="Fechar diff"
        >
          <X className="h-3.5 w-3.5" />
        </Button>
      </div>

      {/* Diff content */}
      <ScrollArea className="min-h-0 flex-1">
        {isEmpty ? (
          <div className="flex flex-col items-center justify-center gap-2 py-16 text-muted-foreground">
            <FileCode className="h-8 w-8 opacity-40" />
            <p className="text-xs">Sem diff disponível para este arquivo.</p>
          </div>
        ) : (
          <pre className="min-w-0 p-3 font-mono text-xs leading-5">
            {lines.map((line, i) => (
              <div
                key={i}
                className={cn(
                  "whitespace-pre px-1",
                  line.type === "add" && "bg-emerald-500/10 text-emerald-700 dark:text-emerald-300",
                  line.type === "del" && "bg-rose-500/10 text-rose-700 dark:text-rose-300",
                  line.type === "hunk" && "text-sky-500 dark:text-sky-400",
                  line.type === "meta" && "text-muted-foreground/60",
                  line.type === "ctx" && "text-foreground/80",
                )}
              >
                {line.content || " "}
              </div>
            ))}
          </pre>
        )}
      </ScrollArea>
    </div>
  );
}
