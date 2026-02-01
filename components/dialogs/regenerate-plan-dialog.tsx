import React, { useState } from "react";
import { Loader2, RefreshCw } from "lucide-react";
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

interface RegeneratePlanDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSubmit: (feedback: string) => Promise<void>;
  isLoading?: boolean;
}

export function RegeneratePlanDialog({
  open,
  onOpenChange,
  onSubmit,
  isLoading = false,
}: RegeneratePlanDialogProps) {
  const [feedback, setFeedback] = useState("");

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = feedback.trim();
    if (!trimmed) return;
    try {
      await onSubmit(trimmed);
      setFeedback("");
      onOpenChange(false);
    } catch {
      // Errors handled by parent
    }
  };

  const handleOpenChange = (next: boolean) => {
    if (!next && !isLoading) {
      setFeedback("");
    }
    onOpenChange(next);
  };

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-[500px]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <RefreshCw className="h-5 w-5" />
            Regenerar plano
          </DialogTitle>
          <DialogDescription>
            Descreva o que precisa ser ajustado no plano atual. O feedback será
            enviado à IA para gerar um novo plano.
          </DialogDescription>
        </DialogHeader>
        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <Label htmlFor="feedback">
              O que precisa ser ajustado no plano? (obrigatório)
            </Label>
            <Textarea
              id="feedback"
              value={feedback}
              onChange={(e) => setFeedback(e.target.value)}
              placeholder="Ex.: Não incluir testes automatizados, Focar só no frontend, A ordem das etapas está invertida..."
              className="mt-2 min-h-[100px] resize-y"
              disabled={isLoading}
              required
            />
          </div>
          <p className="text-xs text-muted-foreground">
            Um novo consumo de tokens/API será utilizado para gerar o plano.
          </p>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => handleOpenChange(false)}
              disabled={isLoading}
            >
              Cancelar
            </Button>
            <Button type="submit" disabled={isLoading || !feedback.trim()}>
              {isLoading ? (
                <>
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  Regenerando...
                </>
              ) : (
                <>
                  <RefreshCw className="mr-2 h-4 w-4" />
                  Regenerar plano
                </>
              )}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
