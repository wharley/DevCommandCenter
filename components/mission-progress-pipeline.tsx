import React from 'react';
import { CheckCircle2, Circle, Loader2, XCircle } from 'lucide-react';
import { cn } from '@/lib/utils';
import type { MissionStatus } from '@/lib/database/types';

interface PipelineStage {
  id: 'plan' | 'code' | 'apply';
  label: string;
  status: 'completed' | 'active' | 'pending' | 'failed';
}

interface MissionProgressPipelineProps {
  status: MissionStatus;
  className?: string;
}

/**
 * Componente visual que mostra o progresso da missão em formato de pipeline
 * Plano → Código → Aplicar
 * 
 * Estados:
 * - completed (verde): Etapa concluída
 * - active (azul animado): Etapa em execução
 * - pending (cinza): Etapa pendente
 * - failed (vermelho): Etapa falhou
 */
export function MissionProgressPipeline({ status, className }: MissionProgressPipelineProps) {
  const stages: PipelineStage[] = React.useMemo(() => {
    // Mapear status da missão para estados do pipeline
    const planStatus =
      status === 'planning'
        ? 'active'
        : ['plan_generated', 'generating_code', 'code_ready', 'applying', 'completed'].includes(status)
        ? 'completed'
        : status === 'failed'
        ? 'failed'
        : 'pending';

    const codeStatus =
      status === 'generating_code'
        ? 'active'
        : ['code_ready', 'applying', 'completed'].includes(status)
        ? 'completed'
        : status === 'failed' && ['generating_code', 'code_ready'].includes(status)
        ? 'failed'
        : planStatus === 'completed'
        ? 'pending'
        : 'pending';

    const applyStatus =
      status === 'applying'
        ? 'active'
        : status === 'completed'
        ? 'completed'
        : status === 'failed' && ['applying', 'completed'].includes(status)
        ? 'failed'
        : codeStatus === 'completed'
        ? 'pending'
        : 'pending';

    return [
      { id: 'plan', label: 'Plano', status: planStatus },
      { id: 'code', label: 'Código', status: codeStatus },
      { id: 'apply', label: 'Aplicar', status: applyStatus },
    ] as PipelineStage[];
  }, [status]);

  const getStageIcon = (stage: PipelineStage) => {
    switch (stage.status) {
      case 'completed':
        return <CheckCircle2 className="h-3.5 w-3.5 text-green-600 dark:text-green-500" />;
      case 'active':
        return <Loader2 className="h-3.5 w-3.5 text-blue-600 dark:text-blue-500 animate-spin" />;
      case 'failed':
        return <XCircle className="h-3.5 w-3.5 text-destructive" />;
      default:
        return <Circle className="h-3.5 w-3.5 text-muted-foreground/50" />;
    }
  };

  const getStageTextColor = (stage: PipelineStage) => {
    switch (stage.status) {
      case 'completed':
        return 'text-green-600 dark:text-green-500 font-medium';
      case 'active':
        return 'text-blue-600 dark:text-blue-500 font-medium';
      case 'failed':
        return 'text-destructive font-medium';
      default:
        return 'text-muted-foreground/70';
    }
  };

  const getConnectorColor = (index: number) => {
    if (index >= stages.length - 1) return '';
    
    const currentStage = stages[index];
    const nextStage = stages[index + 1];
    
    if (currentStage.status === 'completed' && nextStage.status !== 'pending') {
      return 'bg-green-600/30 dark:bg-green-500/30';
    }
    if (currentStage.status === 'active' || nextStage.status === 'active') {
      return 'bg-blue-600/30 dark:bg-blue-500/30';
    }
    return 'bg-muted-foreground/20';
  };

  return (
    <div className={cn('flex items-center gap-1.5', className)}>
      {stages.map((stage, index) => (
        <React.Fragment key={stage.id}>
          <div className="flex items-center gap-1.5">
            {getStageIcon(stage)}
            <span className={cn('text-xs', getStageTextColor(stage))}>
              {stage.label}
            </span>
          </div>
          
          {index < stages.length - 1 && (
            <div className={cn('h-px w-4 transition-colors', getConnectorColor(index))} />
          )}
        </React.Fragment>
      ))}
    </div>
  );
}
