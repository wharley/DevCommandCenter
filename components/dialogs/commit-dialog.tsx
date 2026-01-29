import React, { useEffect, useState } from "react";
import { Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";

interface CommitDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  defaultMessage: string;
  onCommit: (message: string) => Promise<void>;
}

export function CommitDialog({
  open,
  onOpenChange,
  defaultMessage,
  onCommit,
}: CommitDialogProps) {
  const [message, setMessage] = useState(defaultMessage);
  const [isSubmitting, setIsSubmitting] = useState(false);

  useEffect(() => {
    if (open) setMessage(defaultMessage);
  }, [open, defaultMessage]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = message.trim();
    if (!trimmed) return;

    setIsSubmitting(true);
    try {
      await onCommit(trimmed);
      onOpenChange(false);
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[500px]">
        <form onSubmit={handleSubmit}>
          <DialogHeader>
            <DialogTitle>Commitar alterações</DialogTitle>
            <DialogDescription>
              Mensagem do commit. As alterações já aplicadas serão incluídas
              (git add -A).
            </DialogDescription>
          </DialogHeader>
          <div className="grid gap-4 py-4">
            <div className="grid gap-2">
              <Label htmlFor="commit-message">Mensagem</Label>
              <Textarea
                id="commit-message"
                value={message}
                onChange={(e) => setMessage(e.target.value)}
                placeholder="DevCommandCenter: título da missão"
                className="min-h-[100px] resize-none"
                disabled={isSubmitting}
                required
              />
            </div>
          </div>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
              disabled={isSubmitting}
            >
              Cancelar
            </Button>
            <Button type="submit" disabled={isSubmitting || !message.trim()}>
              {isSubmitting ? (
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
              ) : null}
              Commitar
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
