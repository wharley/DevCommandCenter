import type { ReactNode } from "react";
import { ExternalLink, FileDiff, GitBranch } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { WorkspaceCommitButton } from "@/features/commit";
import type { WorkspaceCommitMessageSuggestion } from "@/features/commit/commit-message";
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
    case "complete-merge":
      return "bg-emerald-500/[0.08]";
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
  title?: string | null;
  commitMode: CommitMode;
  isRefreshing?: boolean;
  onCommit?: (message?: string, body?: string | null, stagedFingerprint?: string) => Promise<void> | void;
  onPrepareCommitMessage?: () => Promise<WorkspaceCommitMessageSuggestion>;
  onReviewConflictResolution?: () => void;
  prUrl?: string | null;
  prNumber?: number | null;
  prProvider?: string | null;
  prIsDraft?: boolean;
  identitySlot?: ReactNode;
  hideCommitAction?: boolean;
  suppressCommitButton?: boolean;
  className?: string;
};

export function GitSectionHeader({
  title = "Git",
  commitMode,
  isRefreshing = false,
  onCommit,
  onPrepareCommitMessage,
  onReviewConflictResolution,
  prUrl = null,
  prNumber = null,
  prProvider = null,
  prIsDraft = false,
  identitySlot = null,
  hideCommitAction = false,
  suppressCommitButton = false,
  className,
}: GitSectionHeaderProps) {
  const { t } = useTranslation("common");
  const highlightClass = gitSectionHeaderHighlightClass(commitMode);
  const showPrLink = Boolean(prUrl);
  const prLabel = prProvider === "gitlab" ? "MR" : "PR";

  return (
    <div
      className={cn(
        INSPECTOR_SECTION_HEADER_CLASS,
        "relative gap-2 overflow-hidden border-b-0 shadow-[inset_0_-1px_0_color-mix(in_oklch,var(--border)_60%,transparent)]",
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
      <div className="flex min-w-0 items-center gap-1.5">
        <GitBranch className="size-3.5 shrink-0 text-muted-foreground" strokeWidth={1.8} />
        <span className={cn(INSPECTOR_SECTION_TITLE_CLASS, "min-w-0 truncate translate-y-px")}>
          {title || "Git"}
        </span>
        {showPrLink ? (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-8 shrink-0 gap-1.5 rounded-[9px] px-2.5 text-[12px] font-medium text-muted-foreground hover:text-foreground"
            onClick={() => {
              if (prUrl) {
                void openExternal(prUrl);
              }
            }}
          >
            <ExternalLink className="size-3.5" />
            {prLabel}
            {prNumber ? ` #${prNumber}` : ""}
            {prIsDraft ? <span className="rounded-full bg-muted px-1.5 py-0.5 text-[9px] uppercase tracking-wide">{t("composer.executionDock.draftBadge")}</span> : null}
          </Button>
        ) : null}
        {identitySlot}
      </div>
      <div className="ml-auto flex shrink-0 items-center gap-1.5">
        {commitMode === "complete-merge" && onReviewConflictResolution ? (
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                type="button"
                variant="outline"
                size="icon-sm"
                className="rounded-[9px]"
                aria-label={t("inspector.gitConfirmation.reviewResolution")}
                onClick={onReviewConflictResolution}
              >
                <FileDiff className="size-3.5" />
              </Button>
            </TooltipTrigger>
            <TooltipContent side="bottom">
              {t("inspector.gitConfirmation.reviewResolution")}
            </TooltipContent>
          </Tooltip>
        ) : null}
        {commitMode !== "merged" && !hideCommitAction && !suppressCommitButton ? (
          <WorkspaceCommitButton
            mode={commitMode}
            prProvider={prProvider}
            onCommit={onCommit}
            onPrepareCommitMessage={onPrepareCommitMessage}
          />
        ) : null}
      </div>
    </div>
  );
}
