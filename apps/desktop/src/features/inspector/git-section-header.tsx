import { WorkspaceCommitButton } from "@/features/commit";
import type { CommitMode } from "@/features/commit/WorkspaceCommitButton.logic";
import { INSPECTOR_SECTION_HEADER_CLASS, INSPECTOR_SECTION_TITLE_CLASS } from "@/shell/layout";
import { cn } from "@/lib/utils";

function gitSectionHeaderHighlightClass(mode: CommitMode): string {
  switch (mode) {
    case "fix":
    case "closed":
      return "bg-[var(--workspace-pr-closed-header-bg)]";
    case "resolve-conflicts":
      return "bg-[var(--workspace-pr-conflicts-header-bg)]";
    case "merge":
    case "open-pr":
      return "bg-[var(--workspace-pr-open-header-bg)]";
    case "merged":
      return "bg-[var(--workspace-pr-merged-header-bg)]";
    default:
      return "";
  }
}

export type GitSectionHeaderProps = {
  commitMode: CommitMode;
  isRefreshing?: boolean;
  onCommit?: () => Promise<void> | void;
  className?: string;
};

export function GitSectionHeader({
  commitMode,
  isRefreshing = false,
  onCommit,
  className,
}: GitSectionHeaderProps) {
  const highlightClass = gitSectionHeaderHighlightClass(commitMode);

  return (
    <div
      className={cn(
        INSPECTOR_SECTION_HEADER_CLASS,
        "relative gap-1.5 overflow-hidden border-b-0 shadow-[inset_0_-1px_0_color-mix(in_oklch,var(--border)_60%,transparent)]",
        "transition-[background-color,border-color,color,box-shadow] duration-300 ease-out",
        highlightClass,
        className,
      )}
    >
      {isRefreshing && (
        <div
          data-testid="git-header-shimmer"
          aria-hidden="true"
          className="pointer-events-none absolute inset-x-0 bottom-0 h-px motion-safe:animate-[shine_2s_infinite_linear]"
          style={{
            backgroundImage:
              "linear-gradient(90deg, transparent 0%, transparent 35%, color-mix(in oklch, var(--color-primary) 50%, transparent) 50%, transparent 65%, transparent 100%)",
            backgroundSize: "300% 100%",
          }}
        />
      )}
      <span className={cn(INSPECTOR_SECTION_TITLE_CLASS, "translate-y-px")}>Git</span>
      <WorkspaceCommitButton mode={commitMode} onCommit={onCommit} />
    </div>
  );
}
