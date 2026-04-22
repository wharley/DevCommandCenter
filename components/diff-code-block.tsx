"use client";

import React, { useEffect, useMemo, useRef, useState } from "react";
import { ChevronDown, ChevronUp } from "lucide-react";
import { cn } from "@/lib/utils";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";

type DiffCodeBlockProps = {
  content: string;
  className?: string;
  maxHeightClassName?: string;
};

type DiffSection =
  | {
      kind: "preamble";
      lines: string[];
    }
  | {
      kind: "hunk";
      header: string;
      lines: string[];
    };

function parseDiffSections(content: string): DiffSection[] {
  const lines = content.split(/\r?\n/);
  const sections: DiffSection[] = [];
  let preamble: string[] = [];
  let currentHunk: DiffSection | null = null;

  for (const line of lines) {
    if (line.startsWith("@@")) {
      if (currentHunk) {
        sections.push(currentHunk);
      } else if (preamble.length > 0) {
        sections.push({ kind: "preamble", lines: preamble });
      }
      currentHunk = {
        kind: "hunk",
        header: line,
        lines: [line],
      };
      preamble = [];
      continue;
    }

    if (currentHunk) {
      currentHunk.lines.push(line);
    } else {
      preamble.push(line);
    }
  }

  if (currentHunk) {
    sections.push(currentHunk);
  } else if (preamble.length > 0) {
    sections.push({ kind: "preamble", lines: preamble });
  }

  return sections;
}

function getDiffLineClass(line: string): string {
  if (line.startsWith("+++") || line.startsWith("---")) {
    return "bg-muted/40 text-foreground";
  }
  if (line.startsWith("@@")) {
    return "bg-sky-500/10 text-sky-300";
  }
  if (line.startsWith("+")) {
    return "bg-emerald-500/10 text-emerald-300";
  }
  if (line.startsWith("-")) {
    return "bg-rose-500/10 text-rose-300";
  }
  return "text-muted-foreground";
}

export function DiffCodeBlock({
  content,
  className,
  maxHeightClassName = "max-h-72",
}: DiffCodeBlockProps) {
  const sections = useMemo(() => parseDiffSections(content), [content]);
  const hunkSections = useMemo(
    () => sections.filter((section) => section.kind === "hunk"),
    [sections],
  );
  const [activeHunkIndex, setActiveHunkIndex] = useState(0);
  const hunkRefs = useRef<Array<HTMLElement | null>>([]);

  useEffect(() => {
    setActiveHunkIndex(0);
    hunkRefs.current = [];
  }, [content]);

  useEffect(() => {
    const node = hunkRefs.current[activeHunkIndex];
    if (!node) return;
    node.scrollIntoView({ behavior: "smooth", block: "start" });
  }, [activeHunkIndex, hunkSections]);

  const goToHunk = (delta: number) => {
    if (hunkSections.length === 0) return;
    setActiveHunkIndex((prev) => {
      const next = Math.max(0, Math.min(hunkSections.length - 1, prev + delta));
      return next;
    });
  };

  const isEmpty = content.trim().length === 0;

  let hunkCursor = 0;

  if (isEmpty) {
    return (
      <div className={cn("bg-background/30 px-4 py-6 text-center text-xs text-muted-foreground", className)}>
        Sem conteúdo para exibir.
      </div>
    );
  }

  return (
    <div
      className={cn(
        "overflow-visible bg-background/30",
        maxHeightClassName,
        className,
      )}
    >
      {hunkSections.length > 0 && (
      <div className="sticky top-0 z-10 flex items-center justify-between gap-2 border-b border-border/70 bg-background/95 px-3 py-2 backdrop-blur">
        <div className="flex min-w-0 items-center gap-2">
          <Badge variant="outline" className="shrink-0 text-[10px]">
            {`Hunk ${activeHunkIndex + 1}/${hunkSections.length}`}
          </Badge>
          <span className="truncate text-[10px] text-muted-foreground">
            Navega entre blocos para comparar trechos críticos.
          </span>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          <Button
            type="button"
            variant="outline"
            size="icon-sm"
            onClick={() => goToHunk(-1)}
            disabled={hunkSections.length === 0 || activeHunkIndex <= 0}
            aria-label="Hunk anterior"
          >
            <ChevronUp className="h-3.5 w-3.5" />
          </Button>
          <Button
            type="button"
            variant="outline"
            size="icon-sm"
            onClick={() => goToHunk(1)}
            disabled={
              hunkSections.length === 0 ||
              activeHunkIndex >= hunkSections.length - 1
            }
            aria-label="Próximo hunk"
          >
            <ChevronDown className="h-3.5 w-3.5" />
          </Button>
        </div>
      </div>
      )}

      <div className="w-max min-w-full font-mono text-xs leading-5 whitespace-pre">
        {sections.map((section) => {
          if (section.kind === "preamble") {
            return (
              <div key={`preamble-${section.lines.join("\n")}`} className="border-b border-border/40">
                {section.lines.map((line, index) => (
                  <div
                    key={`preamble-${index}-${line}`}
                    className={cn("px-3 whitespace-pre", getDiffLineClass(line))}
                  >
                    {line.length > 0 ? line : " "}
                  </div>
                ))}
              </div>
            );
          }

          const currentIndex = hunkCursor++;
          const active = currentIndex === activeHunkIndex;
          return (
            <section
              key={`${section.header}-${currentIndex}`}
              ref={(node) => {
                hunkRefs.current[currentIndex] = node;
              }}
              className={cn(
                "scroll-mt-14 border-b border-border/50 last:border-b-0",
                active && "bg-primary/5 ring-1 ring-inset ring-primary/20",
              )}
            >
              {section.lines.map((line, index) => (
                <div
                  key={`${currentIndex}-${index}-${line}`}
                  className={cn(
                    "px-3 whitespace-pre",
                    getDiffLineClass(line),
                    index === 0 &&
                      "sticky top-12 z-[1] border-b border-border/50 bg-background/95 backdrop-blur",
                  )}
                >
                  {line.length > 0 ? line : " "}
                </div>
              ))}
            </section>
          );
        })}
      </div>
    </div>
  );
}
