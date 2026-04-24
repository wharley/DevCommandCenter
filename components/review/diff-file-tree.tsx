"use client";

import React, { useCallback, useEffect, useMemo, useState } from "react";
import { ChevronDown, ChevronRight, FileCode } from "lucide-react";
import { cn } from "@/lib/utils";
import {
  buildDiffFileTree,
  type DiffFileTreeItem,
  type DiffTreeNode,
} from "@/lib/review/diff-file-tree-model";
import { ScrollArea } from "@/components/ui/scroll-area";

function collectDirPaths(nodes: DiffTreeNode[]): string[] {
  const out: string[] = [];
  for (const n of nodes) {
    if (n.kind === "dir") {
      out.push(n.fullPath);
      out.push(...collectDirPaths(n.children));
    }
  }
  return out;
}

type DiffFileTreeProps = {
  files: DiffFileTreeItem[];
  selectedPath: string | null;
  onFileSelect: (path: string) => void;
  className?: string;
  compact?: boolean;
};

function TreeRows(props: {
  nodes: DiffTreeNode[];
  depth: number;
  expanded: Set<string>;
  toggleDir: (fullPath: string) => void;
  selectedPath: string | null;
  onFileSelect: (path: string) => void;
  compact?: boolean;
}) {
  const {
    nodes,
    depth,
    expanded,
    toggleDir,
    selectedPath,
    onFileSelect,
    compact,
  } = props;
  const pad = compact ? 8 + depth * 10 : 10 + depth * 12;

  return (
    <div className="flex flex-col">
      {nodes.map((node) => {
        if (node.kind === "file") {
          const { meta } = node;
          const active = selectedPath === meta.path;
          const ins = meta.insertions ?? 0;
          const del = meta.deletions ?? 0;
          const showDelta = ins > 0 || del > 0;
          return (
            <button
              key={meta.path}
              type="button"
              onClick={() => onFileSelect(meta.path)}
              className={cn(
                "flex w-full items-center gap-1 rounded-md py-1 text-left text-[11px] leading-tight transition-colors",
                "hover:bg-muted/80",
                active && "bg-muted",
              )}
              style={{ paddingLeft: pad, paddingRight: 6 }}
            >
              <FileCode className="h-3 w-3 shrink-0 text-muted-foreground" />
              <span
                className="min-w-0 flex-1 truncate font-mono text-[11px]"
                title={meta.path}
              >
                {node.segment}
              </span>
              {showDelta ? (
                <span
                  className="shrink-0 font-mono text-[10px] text-muted-foreground tabular-nums"
                  title="Inserções / remoções"
                >
                  <span className="text-emerald-600 dark:text-emerald-400">
                    +{ins}
                  </span>
                  <span className="mx-0.5 opacity-40">/</span>
                  <span className="text-rose-600 dark:text-rose-400">
                    −{del}
                  </span>
                </span>
              ) : null}
            </button>
          );
        }

        const isOpen = expanded.has(node.fullPath);
        return (
          <div key={node.fullPath || `root-${node.segment}`} className="flex flex-col">
            <button
              type="button"
              onClick={() => toggleDir(node.fullPath)}
              className={cn(
                "flex w-full items-center gap-0.5 rounded-md py-1 text-left text-[11px] font-medium text-muted-foreground hover:bg-muted/60",
              )}
              style={{ paddingLeft: pad - (compact ? 0 : 2), paddingRight: 6 }}
              aria-expanded={isOpen}
            >
              {isOpen ? (
                <ChevronDown className="h-3.5 w-3.5 shrink-0 opacity-80" />
              ) : (
                <ChevronRight className="h-3.5 w-3.5 shrink-0 opacity-80" />
              )}
              <span className="min-w-0 truncate">{node.segment}</span>
            </button>
            {isOpen ? (
              <TreeRows
                nodes={node.children}
                depth={depth + 1}
                expanded={expanded}
                toggleDir={toggleDir}
                selectedPath={selectedPath}
                onFileSelect={onFileSelect}
                compact={compact}
              />
            ) : null}
          </div>
        );
      })}
    </div>
  );
}

export function DiffFileTree({
  files,
  selectedPath,
  onFileSelect,
  className,
  compact = false,
}: DiffFileTreeProps) {
  const tree = useMemo(() => buildDiffFileTree(files), [files]);

  const pathsKey = useMemo(
    () => files.map((f) => f.path).sort().join("\0"),
    [files],
  );

  const [expanded, setExpanded] = useState<Set<string>>(() => new Set());

  useEffect(() => {
    setExpanded(new Set(collectDirPaths(tree)));
  }, [pathsKey, tree]);

  const toggleDir = useCallback((fullPath: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(fullPath)) next.delete(fullPath);
      else next.add(fullPath);
      return next;
    });
  }, []);

  if (tree.length === 0) {
    return (
      <div
        className={cn(
          "flex shrink-0 flex-col border-border bg-muted/15 text-muted-foreground",
          compact
            ? "w-full border-b md:w-[15rem] xl:w-[17rem] md:border-b-0 md:border-r"
            : "w-full border-b md:w-[16.5rem] xl:w-[19rem] md:border-b-0 md:border-r",
          className,
        )}
      >
        <p className="px-2 py-2 text-[10px]">Sem arquivos na árvore.</p>
      </div>
    );
  }

  return (
    <div
      className={cn(
        "flex min-h-0 shrink-0 flex-col border-border bg-muted/15",
        compact
          ? "max-h-40 w-full border-b md:max-h-none md:w-[15rem] xl:w-[17rem] md:border-b-0 md:border-r"
          : "w-full border-b md:w-[16.5rem] xl:w-[19rem] md:border-b-0 md:border-r",
        className,
      )}
    >
      <div className="shrink-0 border-b border-border/60 px-2 py-1.5">
        <p className="text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
          Arquivos
        </p>
        <p className="text-[10px] text-muted-foreground/80">
          {files.length} alterado(s)
        </p>
      </div>
      <ScrollArea className="min-h-0 flex-1">
        <div className="py-1 pr-2">
          <TreeRows
            nodes={tree}
            depth={0}
            expanded={expanded}
            toggleDir={toggleDir}
            selectedPath={selectedPath}
            onFileSelect={onFileSelect}
            compact={compact}
          />
        </div>
      </ScrollArea>
    </div>
  );
}
