import React from "react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { HelpCircle, Keyboard } from "lucide-react";

const SHORTCUTS = [
  { keys: "⌘ N", description: "Nova missão (na página do projeto)" },
  { keys: "⌘ F", description: "Modo foco (na página da missão)" },
];

interface HelpDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function HelpDialog({ open, onOpenChange }: HelpDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <HelpCircle className="h-5 w-5" />
            Ajuda
          </DialogTitle>
        </DialogHeader>
        <div className="space-y-4 pt-2">
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <Keyboard className="h-4 w-4" />
            <span>Atalhos de teclado</span>
          </div>
          <ul className="space-y-2">
            {SHORTCUTS.map((s) => (
              <li
                key={s.keys}
                className="flex items-center justify-between gap-4 rounded-md border border-border bg-muted/30 px-3 py-2 text-sm"
              >
                <span className="text-muted-foreground">{s.description}</span>
                <kbd className="rounded bg-muted px-2 py-0.5 font-mono text-xs">
                  {s.keys}
                </kbd>
              </li>
            ))}
          </ul>
        </div>
      </DialogContent>
    </Dialog>
  );
}
