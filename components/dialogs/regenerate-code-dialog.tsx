import React, { useState } from "react";
import { Loader2, RotateCcw, AlertCircle } from "lucide-react";
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
import { Alert, AlertDescription } from "@/components/ui/alert";

interface RegenerateCodeDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSubmit: (feedback: string) => Promise<void>;
  isLoading?: boolean;
  attempts?: number;
}

export function RegenerateCodeDialog({
  open,
  onOpenChange,
  onSubmit,
  isLoading = false,
  attempts = 1,
}: RegenerateCodeDialogProps) {
  const [feedback, setFeedback] = useState("");

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    try {
      await onSubmit(feedback.trim());
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

  const isMultipleAttempts = attempts >= 2;
  const isManyAttempts = attempts >= 3;

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-[550px]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <RotateCcw className="h-5 w-5" />
            Regenerar código
            {attempts > 1 && (
              <span className="text-sm font-normal text-muted-foreground">
                (tentativa {attempts})
              </span>
            )}
          </DialogTitle>
          <DialogDescription>
            {isMultipleAttempts
              ? "O código será gerado novamente. Forneça feedback para evitar o mesmo erro."
              : "Forneça contexto opcional sobre o que deu errado para melhorar o resultado."}
          </DialogDescription>
        </DialogHeader>

        {isMultipleAttempts && (
          <Alert variant={isManyAttempts ? "destructive" : "default"}>
            <AlertCircle className="h-4 w-4" />
            <AlertDescription>
              {isManyAttempts ? (
                <>
                  <strong>Múltiplas tentativas detectadas.</strong> Se o
                  problema persistir, considere{" "}
                  <strong>voltar ao plano</strong> e regenerá-lo com feedback.
                  O problema pode estar no plano, não no código.
                </>
              ) : (
                <>
                  Código já gerado {attempts}x. Se continuar errado, o problema
                  pode estar no <strong>plano</strong>.
                </>
              )}
            </AlertDescription>
          </Alert>
        )}

        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <Label htmlFor="code-feedback">
              O que deu errado? {isManyAttempts && <span className="text-destructive">*</span>}
            </Label>
            <Textarea
              id="code-feedback"
              value={feedback}
              onChange={(e) => setFeedback(e.target.value)}
              placeholder="Ex: O código modificou config.ts mas o arquivo correto é settings.ts"
              className="mt-2 min-h-[100px] resize-y"
              disabled={isLoading}
              required={isManyAttempts}
            />
            <p className="text-xs text-muted-foreground mt-2">
              💡 <strong>Dica:</strong> Se o problema está no plano (etapas
              erradas, arquivos incorretos), clique em "Voltar ao plano" e
              regenere o plano com feedback.
            </p>
          </div>
          <p className="text-xs text-muted-foreground">
            Um novo consumo de tokens/API será utilizado para gerar o código.
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
            <Button
              type="submit"
              disabled={isLoading || (isManyAttempts && !feedback.trim())}
            >
              {isLoading ? (
                <>
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  Regenerando...
                </>
              ) : (
                <>
                  <RotateCcw className="mr-2 h-4 w-4" />
                  Regenerar código
                </>
              )}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
