"use client";

import React from "react";
import { Terminal, GitBranch } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

export type WorkflowChoice = "pipeline" | "agents_cli";

interface WorkflowChoiceDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSelect: (choice: WorkflowChoice) => void;
}

export function WorkflowChoiceDialog({
  open,
  onOpenChange,
  onSelect,
}: WorkflowChoiceDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Como quer trabalhar?</DialogTitle>
          <DialogDescription>
            Escolha o fluxo antes de criar a missão ou tarefa.
          </DialogDescription>
        </DialogHeader>
        <div className="grid gap-3 py-2">
          <Button
            type="button"
            variant="outline"
            className="h-auto min-w-0 flex-col items-start gap-2 p-4 text-left whitespace-normal"
            onClick={() => {
              onSelect("pipeline");
              onOpenChange(false);
            }}
          >
            <span className="flex items-center gap-2 font-medium">
              <GitBranch className="h-5 w-5 shrink-0 text-primary" />
              Pipeline
            </span>
            <span className="text-wrap text-sm font-normal text-muted-foreground">
              Plano → Código → Aplicar no app. Gere e revise plano e diff antes de aplicar.
            </span>
          </Button>
          <Button
            type="button"
            variant="outline"
            className="h-auto min-w-0 flex-col items-start gap-2 p-4 text-left whitespace-normal"
            onClick={() => {
              onSelect("agents_cli");
              onOpenChange(false);
            }}
          >
            <span className="flex items-center gap-2 font-medium">
              <Terminal className="h-5 w-5 shrink-0 text-primary" />
              Terminal (agentes)
            </span>
            <span className="text-wrap text-sm font-normal text-muted-foreground">
              Crie tarefas e abra cada uma no terminal com um agente (Codex, Claude, Cursor, etc.). Uma tarefa = um agente = um branch.
            </span>
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
