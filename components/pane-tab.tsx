"use client";

import React, { useState, useRef, useEffect } from "react";
import { Bot, Terminal, Trash2, Edit2, Check, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import type { Pane, Provider } from "@/lib/database/types";
import type { DraggableProvidedDragHandleProps } from "@hello-pangea/dnd";

interface PaneTabProps {
  pane: Pane;
  provider: Provider | null;
  selected: boolean;
  hasUnreadAttention: boolean;
  label: string;
  onSelect: (paneId: string) => void;
  onRemove: (paneId: string) => void;
  onRename: (paneId: string, newTitle: string) => void;
  isDragging?: boolean;
  dragHandleProps?: DraggableProvidedDragHandleProps | null;
}

function cliAgentKindLabel(provider: Provider | null): string {
  const typ = provider?.type;
  if (!typ) return "Agent";
  if (typ === "anthropic") return "Claude";
  if (typ === "openai") return "GPT";
  return typ.charAt(0).toUpperCase() + typ.slice(1);
}

function AgentKindBadge({
  provider,
  compact,
  className,
}: {
  provider: Provider | null;
  compact?: boolean;
  className?: string;
}) {
  const label = cliAgentKindLabel(provider);
  const tip = provider?.name?.trim() ? provider.name : label;
  return (
    <Badge
      variant="secondary"
      title={tip}
      className={`shrink-0 font-normal ${compact ? "h-5 px-1.5 py-0 text-[10px] leading-none" : "text-xs"} ${className ?? ""}`}
    >
      {label}
    </Badge>
  );
}

export function PaneTab({
  pane,
  provider,
  selected,
  hasUnreadAttention,
  label,
  onSelect,
  onRemove,
  onRename,
  isDragging = false,
  dragHandleProps,
}: PaneTabProps) {
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState(label);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (isRenaming && inputRef.current) {
      inputRef.current.focus();
      inputRef.current.select();
    }
  }, [isRenaming]);

  const handleRenameStart = () => {
    setRenameValue(label);
    setIsRenaming(true);
  };

  const handleRenameSave = () => {
    const trimmed = renameValue.trim();
    if (trimmed && trimmed !== label) {
      onRename(pane.id, trimmed);
    }
    setIsRenaming(false);
  };

  const handleRenameCancel = () => {
    setRenameValue(label);
    setIsRenaming(false);
  };

  const handleRenameKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      handleRenameSave();
    } else if (e.key === "Escape") {
      handleRenameCancel();
    }
  };

  const tabContent = (
    <div
      {...(dragHandleProps ?? {})}
      role="tab"
      aria-selected={selected}
      onClick={() => !isRenaming && onSelect(pane.id)}
      className={`group flex min-w-[170px] max-w-[260px] cursor-pointer items-center gap-2 rounded-md border px-2 py-1.5 transition-all ${
        isDragging
          ? "opacity-50 scale-95"
          : selected
          ? "border-primary bg-primary/10"
          : "border-border bg-muted/30 hover:bg-muted/50"
      }`}
      data-pane-id={pane.id}
    >
      {pane.type === "agent" ? (
        <Bot className="h-3.5 w-3.5 shrink-0" />
      ) : (
        <Terminal className="h-3.5 w-3.5 shrink-0" />
      )}

      {pane.type === "agent" ? <AgentKindBadge provider={provider} compact /> : null}

      {isRenaming ? (
        <div className="flex min-w-0 flex-1 items-center gap-1">
          <Input
            ref={inputRef}
            value={renameValue}
            onChange={(e) => setRenameValue(e.target.value)}
            onKeyDown={handleRenameKeyDown}
            onBlur={handleRenameSave}
            className="h-6 min-w-0 flex-1 px-1 text-xs"
            maxLength={50}
            onClick={(e) => e.stopPropagation()}
          />
          <Button
            variant="ghost"
            size="icon"
            className="h-5 w-5 shrink-0"
            onClick={(e) => {
              e.stopPropagation();
              handleRenameSave();
            }}
          >
            <Check className="h-3 w-3" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="h-5 w-5 shrink-0"
            onClick={(e) => {
              e.stopPropagation();
              handleRenameCancel();
            }}
          >
            <X className="h-3 w-3" />
          </Button>
        </div>
      ) : (
        <>
          <span className="min-w-0 flex-1 truncate text-xs">{label}</span>
          {hasUnreadAttention && (
            <span className="h-2 w-2 shrink-0 rounded-full bg-sky-400" />
          )}
          <Button
            variant="ghost"
            size="icon"
            className="ml-auto h-5 w-5 shrink-0 opacity-70 hover:opacity-100"
            onClick={(e) => {
              e.stopPropagation();
              onRemove(pane.id);
            }}
          >
            <Trash2 className="h-3 w-3" />
          </Button>
        </>
      )}
    </div>
  );

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>{tabContent}</ContextMenuTrigger>
      <ContextMenuContent>
        <ContextMenuItem onClick={handleRenameStart}>
          <Edit2 className="mr-2 h-4 w-4" />
          Rename Tab
        </ContextMenuItem>
        <ContextMenuItem
          onClick={() => onRemove(pane.id)}
          className="text-destructive"
        >
          <Trash2 className="mr-2 h-4 w-4" />
          Close Tab
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}
