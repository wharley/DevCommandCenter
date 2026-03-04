import React from "react";
import { Lightbulb } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";

interface MissionTipsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function MissionTipsDialog({ open, onOpenChange }: MissionTipsDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[560px] max-h-[90vh] flex flex-col overflow-hidden gap-4 p-6 min-h-0">
        <DialogHeader className="shrink-0 space-y-1.5">
          <DialogTitle className="flex items-center gap-2">
            <Lightbulb className="h-5 w-5 text-primary" />
            Como tirar mais proveito das missões
          </DialogTitle>
          <DialogDescription>
            Boas práticas para acertar mais, ter mais produtividade e menos retrabalho.
          </DialogDescription>
        </DialogHeader>

        <ScrollArea className="min-h-0 flex-1 max-h-[65vh] overflow-auto pr-4 -mr-4">
          <div className="space-y-5 pb-4 pr-2">
            <section>
              <h3 className="text-sm font-semibold text-foreground mb-1.5">
                1. Pense em missões pequenas (estilo Kanban)
              </h3>
              <p className="text-sm text-muted-foreground mb-2">
                Prefira missões que você conseguiria fazer em 30–90 minutos de trabalho humano.
              </p>
              <p className="text-xs font-medium text-muted-foreground mb-1">Exemplos bons:</p>
              <ul className="text-sm text-muted-foreground list-disc list-inside space-y-0.5 mb-2">
                <li>Adicionar paginação à lista de usuários na tela X.</li>
                <li>Criar testes de unidade para o serviço Y.</li>
                <li>Refatorar o formulário de login para usar React Hook Form.</li>
              </ul>
              <p className="text-xs font-medium text-muted-foreground mb-1">Exemplos ruins (grandes demais):</p>
              <ul className="text-sm text-muted-foreground list-disc list-inside space-y-0.5">
                <li>Reescrever todo o módulo de autenticação.</li>
                <li>Melhorar a performance de todo o sistema.</li>
              </ul>
            </section>

            <Separator />

            <section>
              <h3 className="text-sm font-semibold text-foreground mb-1.5">
                2. Descreva bem o objetivo da missão
              </h3>
              <ul className="text-sm text-muted-foreground space-y-1">
                <li>
                  <strong className="text-foreground">Contexto:</strong> onde no sistema (módulo, tela, serviço).
                </li>
                <li>
                  <strong className="text-foreground">Objetivo:</strong> o que muda para o usuário ou para o sistema.
                </li>
                <li>
                  <strong className="text-foreground">Restrições:</strong> tecnologias, padrões, limites (ex.: não quebrar API pública, manter compatibilidade). Use o campo &quot;Preservar / Não alterar&quot; quando fizer sentido.
                </li>
              </ul>
            </section>

            <Separator />

            <section>
              <h3 className="text-sm font-semibold text-foreground mb-1.5">
                3. Use o plano como seu mini-Kanban
              </h3>
              <p className="text-sm text-muted-foreground mb-2">
                Cada passo do plano deve ser uma tarefa clara e entregável: algo que gera um commit ou mudança verificável.
              </p>
              <p className="text-xs text-muted-foreground">
                Passos típicos: analisar código atual → implementar mudança → escrever/ajustar testes → pequenos ajustes finais.
              </p>
            </section>

            <Separator />

            <section>
              <h3 className="text-sm font-semibold text-foreground mb-1.5">
                4. Geração de código: itere em ciclos curtos
              </h3>
              <p className="text-sm text-muted-foreground mb-2">
                Para cada missão: gerar plano → gerar código → revisar diff → aplicar → testar.
              </p>
              <p className="text-sm text-muted-foreground">
                Evite acumular muitas missões abertas ao mesmo tempo; conclua uma antes de começar outra, como em um board Kanban com WIP baixo.
              </p>
            </section>

            <Separator />

            <section>
              <h3 className="text-sm font-semibold text-foreground mb-1.5">
                5. Revise sempre o que a IA fez
              </h3>
              <p className="text-sm text-muted-foreground mb-2">
                Nunca aplique código sem olhar: use as abas <strong className="text-foreground">Original</strong>, <strong className="text-foreground">Sugerido</strong> e <strong className="text-foreground">Diff</strong> para entender o impacto.
              </p>
              <p className="text-sm text-muted-foreground">
                Use o editor de sugestão para ajustes leves; para refatorações grandes, copie o código para sua IDE, refatore lá e volte com uma nova missão se precisar.
              </p>
            </section>
          </div>
        </ScrollArea>
      </DialogContent>
    </Dialog>
  );
}
