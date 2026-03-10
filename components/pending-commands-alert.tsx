"use client";

import * as React from "react";
import { Terminal, Copy, Check, AlertTriangle } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Checkbox } from "@/components/ui/checkbox";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { PendingCommand } from "@/lib/database/types";

interface PendingCommandsAlertProps {
  commands: PendingCommand[];
  onConfirm: (commandIds: string[]) => void;
  className?: string;
}

export function PendingCommandsAlert({
  commands,
  onConfirm,
  className,
}: PendingCommandsAlertProps) {
  const [copiedId, setCopiedId] = React.useState<string | null>(null);

  const unconfirmedCommands = React.useMemo(
    () => commands.filter((cmd) => !cmd.confirmedAt),
    [commands]
  );

  const allConfirmed = unconfirmedCommands.length === 0;

  const handleCopy = async (command: string, id: string) => {
    try {
      await navigator.clipboard.writeText(command);
      setCopiedId(id);
      setTimeout(() => setCopiedId(null), 2000);
    } catch {
      console.error("Failed to copy command");
    }
  };

  const handleConfirmAll = () => {
    const unconfirmedIds = unconfirmedCommands.map((cmd) => cmd.id);
    onConfirm(unconfirmedIds);
  };

  if (commands.length === 0) {
    return null;
  }

  return (
    <Alert
      className={cn(
        "border-amber-500/50 bg-amber-50/50 dark:bg-amber-950/20",
        allConfirmed && "border-green-500/50 bg-green-50/50 dark:bg-green-950/20",
        className
      )}
    >
      {allConfirmed ? (
        <Check className="h-4 w-4 text-green-600 dark:text-green-400" />
      ) : (
        <AlertTriangle className="h-4 w-4 text-amber-600 dark:text-amber-400" />
      )}
      <AlertTitle
        className={cn(
          "text-amber-800 dark:text-amber-200",
          allConfirmed && "text-green-800 dark:text-green-200"
        )}
      >
        {allConfirmed ? "Comandos Confirmados" : "Comandos Necessários"}
      </AlertTitle>
      <AlertDescription className="mt-2">
        <p
          className={cn(
            "text-amber-700 dark:text-amber-300 mb-3",
            allConfirmed && "text-green-700 dark:text-green-300"
          )}
        >
          {allConfirmed
            ? "Você confirmou que executou todos os comandos."
            : "Os seguintes comandos precisam ser executados manualmente no seu terminal:"}
        </p>

        <div className="space-y-2 mb-4">
          {commands.map((cmd) => (
            <div
              key={cmd.id}
              className={cn(
                "flex items-center gap-2 p-2 rounded-md font-mono text-sm",
                "bg-slate-900 dark:bg-slate-950 text-slate-100",
                cmd.confirmedAt && "opacity-60"
              )}
            >
              <Terminal className="h-4 w-4 text-slate-400 shrink-0" />
              <code className="flex-1 break-all">{cmd.command}</code>
              <Button
                variant="ghost"
                size="icon"
                className="h-7 w-7 text-slate-400 hover:text-slate-200 hover:bg-slate-800 shrink-0"
                onClick={() => handleCopy(cmd.command, cmd.id)}
                title="Copiar comando"
              >
                {copiedId === cmd.id ? (
                  <Check className="h-3.5 w-3.5 text-green-400" />
                ) : (
                  <Copy className="h-3.5 w-3.5" />
                )}
              </Button>
              {cmd.confirmedAt && (
                <Check className="h-4 w-4 text-green-500 shrink-0" />
              )}
            </div>
          ))}
        </div>

        {!allConfirmed && (
          <div className="flex items-center gap-2">
            <Checkbox
              id="confirm-commands"
              checked={allConfirmed}
              onCheckedChange={(checked) => {
                if (checked) {
                  handleConfirmAll();
                }
              }}
            />
            <label
              htmlFor="confirm-commands"
              className="text-sm font-medium text-amber-800 dark:text-amber-200 cursor-pointer select-none"
            >
              Confirmo que executei os comandos acima
            </label>
          </div>
        )}
      </AlertDescription>
    </Alert>
  );
}

interface PendingCommandsBadgeProps {
  count: number;
  confirmed: boolean;
  className?: string;
}

export function PendingCommandsBadge({
  count,
  confirmed,
  className,
}: PendingCommandsBadgeProps) {
  if (count === 0) return null;

  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium",
        confirmed
          ? "bg-green-100 text-green-800 dark:bg-green-900/30 dark:text-green-300"
          : "bg-amber-100 text-amber-800 dark:bg-amber-900/30 dark:text-amber-300",
        className
      )}
    >
      <Terminal className="h-3 w-3" />
      {count} comando{count > 1 ? "s" : ""}
      {confirmed && <Check className="h-3 w-3" />}
    </span>
  );
}
