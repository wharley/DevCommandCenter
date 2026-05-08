import { ChevronsRight, ExternalLink, RotateCcw } from "lucide-react";
import { WorkspaceCommitButton } from "@/features/commit";
import type { CommitMode } from "@/features/commit/WorkspaceCommitButton.logic";
import { INSPECTOR_SECTION_HEADER_CLASS, INSPECTOR_SECTION_TITLE_CLASS } from "@/shell/layout";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { openExternal } from "@/lib/shell-api";

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
  onContinueWorkspace?: () => Promise<void> | void;
  isContinuingWorkspace?: boolean;
  onRetrySetup?: () => Promise<void> | void;
  isRetryingSetup?: boolean;
  showRetrySetup?: boolean;
  retrySetupLabel?: string;
  prUrl?: string | null;
  prNumber?: number | null;
  prProvider?: string | null;
  className?: string;
};

export function GitSectionHeader({
  commitMode,
  isRefreshing = false,
  onCommit,
  onContinueWorkspace,
  isContinuingWorkspace = false,
  onRetrySetup,
  isRetryingSetup = false,
  showRetrySetup = false,
  retrySetupLabel = "Retry setup",
  prUrl = null,
  prNumber = null,
  prProvider = null,
  className,
}: GitSectionHeaderProps) {
  const highlightClass = gitSectionHeaderHighlightClass(commitMode);
  const showContinue = commitMode === "merged" && Boolean(onContinueWorkspace);
  const showPrLink = Boolean(prUrl);
  const prLabel = prProvider === "gitlab" ? "MR" : "PR";

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
      {showPrLink ? (
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="h-8 gap-1.5 rounded-[9px] px-2.5 text-[12px] font-medium text-muted-foreground hover:text-foreground"
          onClick={() => {
            if (prUrl) {
              void openExternal(prUrl);
            }
          }}
        >
          <ExternalLink className="size-3.5" />
          {prLabel}
          {prNumber ? ` #${prNumber}` : ""}
        </Button>
      ) : null}
      {showContinue ? (
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="h-8 gap-1.5 rounded-[9px] px-2.5 text-[12px] font-medium"
          disabled={isContinuingWorkspace}
          onClick={() => {
            void onContinueWorkspace?.();
          }}
        >
          <ChevronsRight className="size-3.5" />
          Continue
        </Button>
      ) : null}
      {showRetrySetup ? (
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="h-8 gap-1.5 rounded-[9px] px-2.5 text-[12px] font-medium"
          disabled={isRetryingSetup}
          onClick={() => {
            void onRetrySetup?.();
          }}
        >
          <RotateCcw className="size-3.5" />
          {retrySetupLabel}
        </Button>
      ) : null}
      {!showContinue ? <WorkspaceCommitButton mode={commitMode} onCommit={onCommit} /> : null}
    </div>
  );
}
