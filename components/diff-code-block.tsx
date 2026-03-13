import React from "react";
import { cn } from "@/lib/utils";

type DiffCodeBlockProps = {
  content: string;
  className?: string;
  maxHeightClassName?: string;
};

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
  const lines = content.split(/\r?\n/);

  return (
    <div className={cn("overflow-auto", maxHeightClassName, className)}>
      <pre className="font-mono text-xs leading-5 whitespace-pre-wrap">
        {lines.map((line, index) => (
          <div key={`${index}-${line}`} className={cn("px-3", getDiffLineClass(line))}>
            {line.length > 0 ? line : " "}
          </div>
        ))}
      </pre>
    </div>
  );
}
