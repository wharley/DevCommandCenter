/**
 * AI Orchestrator - Orquestrador central para operações de IA
 *
 * Gerencia a seleção de adapters, contexto do projeto e execução de missões
 */

import { createAdapter } from "./adapters";
import { GitService } from "./git-service";
import db from "../../lib/database";
import { providerService } from "./provider-service";
import type {
  AIProviderAdapter,
  AIResponse,
  ProjectContext,
  AdapterConfig,
  MissionPlan,
  GeneratedCode,
  ApplyChangesResult,
  Provider,
  Mission,
} from "./types";

export class AIOrchestrator {
  private adaptersCache: Map<string, AIProviderAdapter> = new Map();

  /**
   * Obtém ou cria um adapter para o provider
   */
  private getAdapter(provider: Provider): AIProviderAdapter {
    const cached = this.adaptersCache.get(provider.id);
    if (cached) return cached;

    const adapter = createAdapter(provider);
    this.adaptersCache.set(provider.id, adapter);
    return adapter;
  }

  /**
   * Invalida o cache de um adapter (útil quando o provider é atualizado)
   */
  invalidateAdapter(providerId: string): void {
    this.adaptersCache.delete(providerId);
  }

  /**
   * Obtém o contexto completo de um projeto
   */
  async getProjectContext(
    projectPath: string,
    projectName: string
  ): Promise<ProjectContext> {
    const gitService = new GitService(projectPath);

    // Obtém informações do Git e lista de arquivos em paralelo
    const [gitInfo, files] = await Promise.all([
      gitService.getGitInfo(),
      gitService.listTrackedFiles(200),
    ]);

    return {
      projectPath,
      projectName,
      files,
      gitInfo: gitInfo || undefined,
    };
  }

  /**
   * Gera um plano de ação para uma missão
   * @param options.planFeedback Feedback do usuário ao regenerar (o que ajustar no plano anterior)
   */
  async generatePlan(
    missionId: string,
    options?: { planFeedback?: string }
  ): Promise<AIResponse<MissionPlan>> {
    // Timestamp do último log de progresso para evitar spam
    let lastProgressLog = 0;
    const progressLogThrottleMs = 5000; // Log de progresso a cada 5 segundos no máximo

    try {
      // Busca a missão e o projeto
      const mission = db.missions.findById(missionId);
      if (!mission) {
        return { success: false, error: "Mission not found" };
      }

      // Uma missão por projeto por vez (evita conflitos Git)
      const othersInProgress = db.missions.findInProgress(
        mission.projectId,
        missionId
      );
      if (othersInProgress.length > 0) {
        const other = othersInProgress[0];
        return {
          success: false,
          error: `Já existe uma missão em andamento neste projeto ("${other.title}"). Aguarde a conclusão ou cancele a missão atual.`,
        };
      }

      const project = db.projects.findById(mission.projectId);
      if (!project) {
        return { success: false, error: "Project not found" };
      }

      // Determina o provider a usar (plan: planProviderId > providerId > default)
      const providerId =
        mission.planProviderId ||
        mission.providerId ||
        project.defaultProviderId;
      if (!providerId) {
        return {
          success: false,
          error: "No provider configured for this mission or project",
        };
      }

      const provider = providerService.findById(providerId);
      if (!provider) {
        return { success: false, error: "Provider not found" };
      }

      if (!provider.isActive) {
        return { success: false, error: "Provider is not active" };
      }

      // Obtém o adapter e valida
      const adapter = this.getAdapter(provider);
      const validation = adapter.validate();
      if (!validation.valid) {
        return { success: false, error: validation.errors.join("; ") };
      }

      // Obtém contexto do projeto
      const projectContext = await this.getProjectContext(
        project.path,
        project.name
      );

      // Configura e executa
      const config: AdapterConfig = {
        provider,
        mission,
        projectContext,
        planFeedback: options?.planFeedback,
      };

      // Atualiza status para "planning"
      db.missions.updateStatus(missionId, "planning");
      db.missionLogs.logInfo(
        missionId,
        `Starting plan generation with ${adapter.name}`
      );

      // Callback de progresso que loga mensagens de status
      const onProgress = (message: string) => {
        const now = Date.now();
        // Evita spam de logs - só loga a cada 5 segundos
        if (now - lastProgressLog >= progressLogThrottleMs) {
          lastProgressLog = now;
          db.missionLogs.logInfo(missionId, message);
        }
      };

      const result = await adapter.generatePlan(config, onProgress);

      if (result.success && result.data) {
        // Salva o plano e atualiza status
        db.missions.updatePlan(missionId, result.data);
        db.missions.updateStatus(missionId, "plan_generated");
        db.missionLogs.logInfo(
          missionId,
          "Plan generated successfully",
          result.metadata as Record<string, unknown>
        );
      } else {
        // Log do erro
        db.missionLogs.logError(
          missionId,
          result.error || "Unknown error",
          result.metadata as Record<string, unknown>
        );
      }

      return result;
    } catch (error) {
      const errorMessage =
        error instanceof Error ? error.message : "Unknown error";
      db.missionLogs.logError(missionId, errorMessage);
      return { success: false, error: errorMessage };
    }
  }

  /**
   * Gera código para uma missão
   */
  async generateCode(
    missionId: string,
    options?: { codeFeedback?: string }
  ): Promise<AIResponse<GeneratedCode>> {
    // Timestamp do último log de progresso para evitar spam
    let lastProgressLog = 0;
    const progressLogThrottleMs = 2000; // Log a cada 2s (front faz polling a cada 2.5s)

    try {
      // Busca a missão e o projeto
      const mission = db.missions.findById(missionId);
      if (!mission) {
        return { success: false, error: "Mission not found" };
      }

      // Uma missão por projeto por vez (evita conflitos Git)
      const othersInProgress = db.missions.findInProgress(
        mission.projectId,
        missionId
      );
      if (othersInProgress.length > 0) {
        const other = othersInProgress[0];
        return {
          success: false,
          error: `Já existe uma missão em andamento neste projeto ("${other.title}"). Aguarde a conclusão ou cancele a missão atual.`,
        };
      }

      if (!mission.plan) {
        return {
          success: false,
          error: "Mission has no plan. Generate a plan first.",
        };
      }

      const project = db.projects.findById(mission.projectId);
      if (!project) {
        return { success: false, error: "Project not found" };
      }

      // Determina o provider a usar (code: codeProviderId > providerId > default)
      const providerId =
        mission.codeProviderId ||
        mission.providerId ||
        project.defaultProviderId;
      if (!providerId) {
        return {
          success: false,
          error: "No provider configured for this mission or project",
        };
      }

      const provider = providerService.findById(providerId);
      if (!provider) {
        return { success: false, error: "Provider not found" };
      }

      if (!provider.isActive) {
        return { success: false, error: "Provider is not active" };
      }

      // Obtém o adapter e valida
      const adapter = this.getAdapter(provider);
      const validation = adapter.validate();
      if (!validation.valid) {
        return { success: false, error: validation.errors.join("; ") };
      }

      // Obtém contexto do projeto
      const projectContext = await this.getProjectContext(
        project.path,
        project.name
      );

      // Configura e executa
      const config: AdapterConfig = {
        provider,
        mission,
        projectContext,
        ...(options?.codeFeedback && { codeFeedback: options.codeFeedback }),
      };

      // Atualiza status para "generating_code"
      db.missions.updateStatus(missionId, "generating_code");
      db.missionLogs.logInfo(
        missionId,
        `Starting code generation with ${adapter.name}`
      );

      // Callback de progresso que loga mensagens de status
      const onProgress = (message: string) => {
        const now = Date.now();
        if (now - lastProgressLog >= progressLogThrottleMs) {
          lastProgressLog = now;
          db.missionLogs.logInfo(missionId, message);
        }
      };

      const result = await adapter.generateCode(config, onProgress);

      if (result.success && result.data) {
        // Salva o código e atualiza status
        db.missions.updateGeneratedCode(missionId, result.data);
        db.missions.updateStatus(missionId, "code_ready");
        // Marca todas as etapas do plano como concluídas (progress 6/6)
        const mission = db.missions.findById(missionId);
        if (mission?.plan?.steps?.length) {
          const steps = mission.plan.steps.map((s) => ({
            ...s,
            status: "completed" as const,
          }));
          db.missions.updatePlan(missionId, { ...mission.plan, steps });
        }
        db.missionLogs.logInfo(
          missionId,
          `Code generated: ${result.data.files?.length || 0} files`,
          result.metadata as Record<string, unknown>
        );
      } else {
        // Log do erro
        db.missionLogs.logError(
          missionId,
          result.error || "Unknown error",
          result.metadata as Record<string, unknown>
        );
      }

      return result;
    } catch (error) {
      const errorMessage =
        error instanceof Error ? error.message : "Unknown error";
      db.missionLogs.logError(missionId, errorMessage);
      return { success: false, error: errorMessage };
    }
  }

  /**
   * Aplica as mudanças geradas ao projeto
   */
  async applyChanges(
    missionId: string,
    options: {
      createBackup?: boolean;
      dryRun?: boolean;
      filePaths?: string[];
      editedContent?: Record<string, string>;
    } = {}
  ): Promise<ApplyChangesResult> {
    try {
      // Busca a missão e o projeto
      const mission = db.missions.findById(missionId);
      if (!mission) {
        return {
          success: false,
          appliedFiles: [],
          failedFiles: [{ path: "", error: "Mission not found" }],
        };
      }

      // Uma missão por projeto por vez (evita conflitos Git)
      const othersInProgress = db.missions.findInProgress(
        mission.projectId,
        missionId
      );
      if (othersInProgress.length > 0) {
        const other = othersInProgress[0];
        return {
          success: false,
          appliedFiles: [],
          failedFiles: [
            {
              path: "",
              error: `Já existe uma missão em andamento neste projeto ("${other.title}"). Aguarde a conclusão ou cancele a missão atual.`,
            },
          ],
        };
      }

      if (!mission.generatedCode?.files?.length) {
        return {
          success: false,
          appliedFiles: [],
          failedFiles: [{ path: "", error: "No code to apply" }],
        };
      }

      const project = db.projects.findById(mission.projectId);
      if (!project) {
        return {
          success: false,
          appliedFiles: [],
          failedFiles: [{ path: "", error: "Project not found" }],
        };
      }

      const requestedPaths = options.filePaths;
      let filesToApply =
        requestedPaths && requestedPaths.length > 0
          ? mission.generatedCode.files.filter((f) =>
              requestedPaths.includes(f.path)
            )
          : mission.generatedCode.files;

      if (
        options.editedContent &&
        Object.keys(options.editedContent).length > 0
      ) {
        filesToApply = filesToApply.map((f) => {
          const edited = options.editedContent![f.path];
          if (edited !== undefined) {
            return {
              ...f,
              suggestedContent: edited,
              diff: undefined,
            };
          }
          return f;
        });
      }

      if (filesToApply.length === 0) {
        return {
          success: false,
          appliedFiles: [],
          failedFiles: [
            {
              path: "",
              error: "Nenhum arquivo selecionado para aplicar.",
            },
          ],
        };
      }

      // Atualiza status para "applying"
      db.missions.updateStatus(missionId, "applying");
      db.missionLogs.logAction(
        missionId,
        `Applying ${filesToApply.length} file changes`
      );

      // Aplica as mudanças
      const gitService = new GitService(project.path);
      const result = await gitService.applyChanges(filesToApply, {
        createBackup: options.createBackup ?? true,
        dryRun: options.dryRun ?? false,
      });

      if (result.success) {
        db.missions.complete(
          missionId,
          `Applied ${result.appliedFiles.length} files successfully`
        );
        // Marca como committed (applyChanges já faz commit automático)
        db.missions.update(missionId, { isCommitted: true });
        db.missionLogs.logAction(missionId, "Changes applied successfully", {
          appliedFiles: result.appliedFiles,
          backupPath: result.backupPath,
        });
      } else {
        const errMsg =
          result.failedFiles.length > 0
            ? `Some files failed to apply: ${result.failedFiles
                .map((f) => `${f.path}: ${f.error}`)
                .join("; ")}`
            : "Some files failed to apply";
        db.missionLogs.logError(missionId, "Some files failed to apply", {
          appliedFiles: result.appliedFiles,
          failedFiles: result.failedFiles,
        });
        db.missions.fail(missionId, errMsg);
      }

      return result;
    } catch (error) {
      const errorMessage =
        error instanceof Error ? error.message : "Unknown error";
      db.missionLogs.logError(missionId, errorMessage);
      db.missions.fail(missionId, errorMessage);
      return {
        success: false,
        appliedFiles: [],
        failedFiles: [{ path: "", error: errorMessage }],
      };
    }
  }

  /**
   * Testa a conexão com um provider
   */
  async testProviderConnection(
    providerId: string
  ): Promise<{ success: boolean; message: string }> {
    const provider = providerService.findById(providerId);
    if (!provider) {
      return { success: false, message: "Provider not found" };
    }

    try {
      const adapter = this.getAdapter(provider);
      return await adapter.testConnection();
    } catch (error) {
      return {
        success: false,
        message: error instanceof Error ? error.message : "Unknown error",
      };
    }
  }

  /**
   * Valida um provider sem persistir
   */
  validateProvider(provider: Provider): {
    valid: boolean;
    errors: string[];
    warnings?: string[];
  } {
    try {
      const adapter = createAdapter(provider);
      return adapter.validate();
    } catch (error) {
      return {
        valid: false,
        errors: [error instanceof Error ? error.message : "Unknown error"],
      };
    }
  }
}

// Singleton instance
export const aiOrchestrator = new AIOrchestrator();

// Factory function
export function createAIOrchestrator(): AIOrchestrator {
  return new AIOrchestrator();
}
