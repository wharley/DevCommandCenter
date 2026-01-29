/**
 * AI Orchestrator - Orquestrador central para operações de IA
 *
 * Gerencia a seleção de adapters, contexto do projeto e execução de missões
 */

import { createAdapter } from "./adapters";
import { GitService } from "./git-service";
import db from "../../lib/database";
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
    projectName: string,
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
   */
  async generatePlan(missionId: string): Promise<AIResponse<MissionPlan>> {
    // Timestamp do último log de progresso para evitar spam
    let lastProgressLog = 0;
    const progressLogThrottleMs = 5000; // Log de progresso a cada 5 segundos no máximo

    try {
      // Busca a missão e o projeto
      const mission = db.missions.findById(missionId);
      if (!mission) {
        return { success: false, error: "Mission not found" };
      }

      const project = db.projects.findById(mission.projectId);
      if (!project) {
        return { success: false, error: "Project not found" };
      }

      // Determina o provider a usar
      const providerId = mission.providerId || project.defaultProviderId;
      if (!providerId) {
        return {
          success: false,
          error: "No provider configured for this mission or project",
        };
      }

      const provider = db.providers.findById(providerId);
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
        project.name,
      );

      // Configura e executa
      const config: AdapterConfig = {
        provider,
        mission,
        projectContext,
      };

      // Atualiza status para "planning"
      db.missions.updateStatus(missionId, "planning");
      db.missionLogs.logInfo(
        missionId,
        `Starting plan generation with ${adapter.name}`,
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
          result.metadata as Record<string, unknown>,
        );
      } else {
        // Log do erro
        db.missionLogs.logError(
          missionId,
          result.error || "Unknown error",
          result.metadata as Record<string, unknown>,
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
  async generateCode(missionId: string): Promise<AIResponse<GeneratedCode>> {
    // Timestamp do último log de progresso para evitar spam
    let lastProgressLog = 0;
    const progressLogThrottleMs = 5000; // Log de progresso a cada 5 segundos no máximo

    try {
      // Busca a missão e o projeto
      const mission = db.missions.findById(missionId);
      if (!mission) {
        return { success: false, error: "Mission not found" };
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

      // Determina o provider a usar
      const providerId = mission.providerId || project.defaultProviderId;
      if (!providerId) {
        return {
          success: false,
          error: "No provider configured for this mission or project",
        };
      }

      const provider = db.providers.findById(providerId);
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
        project.name,
      );

      // Configura e executa
      const config: AdapterConfig = {
        provider,
        mission,
        projectContext,
      };

      // Atualiza status para "generating_code"
      db.missions.updateStatus(missionId, "generating_code");
      db.missionLogs.logInfo(
        missionId,
        `Starting code generation with ${adapter.name}`,
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
          result.metadata as Record<string, unknown>,
        );
      } else {
        // Log do erro
        db.missionLogs.logError(
          missionId,
          result.error || "Unknown error",
          result.metadata as Record<string, unknown>,
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
    options: { createBackup?: boolean; dryRun?: boolean } = {},
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

      // Atualiza status para "applying"
      db.missions.updateStatus(missionId, "applying");
      db.missionLogs.logAction(
        missionId,
        `Applying ${mission.generatedCode.files.length} file changes`,
      );

      // Aplica as mudanças
      const gitService = new GitService(project.path);
      const result = await gitService.applyChanges(
        mission.generatedCode.files,
        {
          createBackup: options.createBackup ?? true,
          dryRun: options.dryRun ?? false,
        },
      );

      if (result.success) {
        db.missions.complete(
          missionId,
          `Applied ${result.appliedFiles.length} files successfully`,
        );
        db.missionLogs.logAction(missionId, "Changes applied successfully", {
          appliedFiles: result.appliedFiles,
          backupPath: result.backupPath,
        });
      } else {
        const errMsg =
          result.failedFiles.length > 0
            ? `Some files failed to apply: ${result.failedFiles.map((f) => `${f.path}: ${f.error}`).join("; ")}`
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
    providerId: string,
  ): Promise<{ success: boolean; message: string }> {
    const provider = db.providers.findById(providerId);
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
