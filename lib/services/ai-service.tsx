// Dev Command Center - AI Service
// Serviço para comunicação com providers de IA
// Usa IPC para o backend nativo (Tauri) ou mock no browser

import type {
  Provider,
  Mission,
  MissionPlan,
  GeneratedCode,
  PlanStep,
  CodeSuggestion,
} from "@/lib/database/types";

// ============================================
// Tipos do serviço
// ============================================

export interface AIServiceConfig {
  provider: Provider;
  mission: Mission;
  projectContext?: {
    files: string[];
    fileContents?: Record<string, string>;
  };
  codeFeedback?: string;
}

export interface AIResponse<T = MissionPlan | GeneratedCode> {
  success: boolean;
  data?: T;
  error?: string;
  metadata?: {
    tokensUsed?: number;
    durationMs?: number;
    model?: string;
    provider?: string;
  };
}

export interface ApplyChangesResult {
  success: boolean;
  appliedFiles: string[];
  failedFiles: Array<{ path: string; error: string }>;
  backupPath?: string;
  appliedVia?: Array<{ path: string; via: "git-apply" | "file-write" }>;
}

export interface ValidationResult {
  valid: boolean;
  errors: string[];
  warnings?: string[];
}

// ============================================
// Shell desktop com bridge de IA
// ============================================

const isDesktopAi = () =>
  typeof window !== "undefined" && !!window.desktopAPI?.ai;

// ============================================
// Simulação de respostas (para browser/dev)
// ============================================

const delay = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

const generateMockPlan = (mission: Mission): MissionPlan => {
  const baseSteps: Omit<PlanStep, "id">[] = [
    {
      order: 1,
      title: "Analyze current implementation",
      description: `Review existing code related to: ${mission.title}`,
      files: ["src/components/", "src/lib/"],
      status: "pending",
    },
    {
      order: 2,
      title: "Design solution architecture",
      description: "Create technical design and identify affected components",
      status: "pending",
    },
    {
      order: 3,
      title: "Implement core changes",
      description: mission.description.slice(0, 100),
      files: ["src/"],
      status: "pending",
    },
    {
      order: 4,
      title: "Update related components",
      description: "Modify dependent components and update imports",
      status: "pending",
    },
    {
      order: 5,
      title: "Add tests and documentation",
      description: "Write unit tests and update documentation",
      files: ["__tests__/", "docs/"],
      status: "pending",
    },
  ];

  return {
    summary: `Implementation plan for: ${mission.title}. This plan covers analysis, implementation, and testing phases.`,
    estimatedComplexity:
      mission.description.length > 200
        ? "high"
        : mission.description.length > 100
        ? "medium"
        : "low",
    steps: baseSteps.map((step, index) => ({
      ...step,
      id: `step-${Date.now()}-${index}`,
    })),
  };
};

const generateMockCode = (mission: Mission): GeneratedCode => {
  const suggestions: CodeSuggestion[] = [
    {
      path: "src/components/NewFeature.tsx",
      action: "create",
      suggestedContent: `// Generated component for: ${mission.title}
import React from 'react';

export function NewFeature() {
  return (
    <div className="p-4">
      <h2 className="text-xl font-bold">New Feature</h2>
      <p>Implementation based on: ${mission.description.slice(0, 50)}...</p>
    </div>
  );
}
`,
      diff: `+++ src/components/NewFeature.tsx
@@ -0,0 +1,12 @@
+// Generated component for: ${mission.title}
+import React from 'react';
+
+export function NewFeature() {
+  return (
+    <div className="p-4">
+      <h2 className="text-xl font-bold">New Feature</h2>
+      <p>Implementation based on mission description...</p>
+    </div>
+  );
+}
`,
    },
    {
      path: "src/lib/utils.ts",
      action: "modify",
      originalContent: `export function cn(...classes: string[]) {
  return classes.filter(Boolean).join(' ');
}
`,
      suggestedContent: `export function cn(...classes: string[]) {
  return classes.filter(Boolean).join(' ');
}

// New utility for: ${mission.title}
export function newHelper() {
  // TODO: Implement based on requirements
  return true;
}
`,
      diff: `--- src/lib/utils.ts
+++ src/lib/utils.ts
@@ -1,3 +1,10 @@
 export function cn(...classes: string[]) {
   return classes.filter(Boolean).join(' ');
 }
+
+// New utility for: ${mission.title}
+export function newHelper() {
+  // TODO: Implement based on requirements
+  return true;
+}
`,
    },
  ];

  return {
    summary: `Generated ${suggestions.length} file changes for: ${mission.title}`,
    files: suggestions,
  };
};

// ============================================
// Serviço de IA
// ============================================

export class AIService {
  private config: AIServiceConfig;

  constructor(config: AIServiceConfig) {
    this.config = config;
  }

  /**
   * Gera um plano de ação para a missão
   * No app desktop (Tauri), usa IPC para o backend nativo.
   * No browser, usa mock.
   */
  async generatePlan(planFeedback?: string): Promise<AIResponse<MissionPlan>> {
    // Backend real quando o bridge de IA está disponível
    if (isDesktopAi() && window.desktopAPI?.ai) {
      try {
        const result = await window.desktopAPI.ai.generatePlan(
          this.config.mission.id,
          planFeedback ? { planFeedback } : undefined
        );
        return result as AIResponse<MissionPlan>;
      } catch (error) {
        return {
          success: false,
          error:
            error instanceof Error
              ? error.message
              : "Unknown error calling AI service",
        };
      }
    }

    // Fallback para mock (browser / sem bridge)
    const startTime = Date.now();

    try {
      // Simula tempo de processamento
      await delay(2000 + Math.random() * 1000);

      const plan = generateMockPlan(this.config.mission);

      return {
        success: true,
        data: plan,
        metadata: {
          tokensUsed: 800 + Math.floor(Math.random() * 500),
          durationMs: Date.now() - startTime,
          model: (this.config.provider.config?.model as string) || "mock",
          provider: "Mock (Browser)",
        },
      };
    } catch (error) {
      return {
        success: false,
        error:
          error instanceof Error
            ? error.message
            : "Unknown error generating plan",
      };
    }
  }

  /**
   * Gera sugestões de código baseadas no plano
   * No app desktop (Tauri), usa IPC para o backend nativo.
   * No browser, usa mock.
   */
  async generateCode(): Promise<AIResponse<GeneratedCode>> {
    // Backend real quando o bridge de IA está disponível
    if (isDesktopAi() && window.desktopAPI?.ai) {
      try {
        const result = await window.desktopAPI.ai.generateCode(
          this.config.mission.id,
          this.config.codeFeedback
            ? { codeFeedback: this.config.codeFeedback }
            : undefined
        );
        return result as AIResponse<GeneratedCode>;
      } catch (error) {
        return {
          success: false,
          error:
            error instanceof Error
              ? error.message
              : "Unknown error calling AI service",
        };
      }
    }

    // Fallback para mock (browser / sem bridge)
    const startTime = Date.now();

    try {
      // Simula tempo de processamento maior
      await delay(3000 + Math.random() * 2000);

      const code = generateMockCode(this.config.mission);

      return {
        success: true,
        data: code,
        metadata: {
          tokensUsed: 1500 + Math.floor(Math.random() * 1000),
          durationMs: Date.now() - startTime,
          model: (this.config.provider.config?.model as string) || "mock",
          provider: "Mock (Browser)",
        },
      };
    } catch (error) {
      return {
        success: false,
        error:
          error instanceof Error
            ? error.message
            : "Unknown error generating code",
      };
    }
  }

  /**
   * Aplica as mudanças ao projeto
   * Requer o app desktop com bridge de IA.
   */
  async applyChanges(options?: {
    createBackup?: boolean;
    dryRun?: boolean;
    filePaths?: string[];
    editedContent?: Record<string, string>;
  }): Promise<ApplyChangesResult> {
    if (isDesktopAi() && window.desktopAPI?.ai) {
      try {
        return await window.desktopAPI.ai.applyChanges(
          this.config.mission.id,
          options
        );
      } catch (error) {
        return {
          success: false,
          appliedFiles: [],
          failedFiles: [
            {
              path: "",
              error: error instanceof Error ? error.message : "Unknown error",
            },
          ],
        };
      }
    }

    // Mock para browser
    await delay(1000);
    return {
      success: true,
      appliedFiles: ["src/components/NewFeature.tsx", "src/lib/utils.ts"],
      failedFiles: [],
      backupPath: "/mock/backup/path",
    };
  }

  /**
   * Verifica se o provider está configurado corretamente
   */
  static validateProvider(provider: Provider): ValidationResult {
    const errors: string[] = [];
    const warnings: string[] = [];

    if (!provider.name) {
      errors.push("Provider name is required");
    }

    if (provider.type === "openai" || provider.type === "anthropic") {
      if (!provider.apiKey) {
        errors.push("API key is required for this provider");
      }
    }

    if (provider.type === "claude-code" || provider.type === "codex") {
      if (!provider.cliPath) {
        errors.push(`CLI path is required for ${provider.type}`);
      }
    }

    return {
      valid: errors.length === 0,
      errors,
      warnings,
    };
  }

  /**
   * Valida o provider usando o backend (mais completo)
   */
  static async validateProviderAsync(
    provider: Provider
  ): Promise<ValidationResult> {
    if (isDesktopAi() && window.desktopAPI?.ai) {
      try {
        return await window.desktopAPI.ai.validateProvider(provider);
      } catch (error) {
        return {
          valid: false,
          errors: [error instanceof Error ? error.message : "Unknown error"],
        };
      }
    }
    return AIService.validateProvider(provider);
  }

  /**
   * Testa a conexão com o provider
   */
  static async testConnection(
    providerId: string
  ): Promise<{ success: boolean; message: string }> {
    if (isDesktopAi() && window.desktopAPI?.ai) {
      try {
        return await window.desktopAPI.ai.testConnection(providerId);
      } catch (error) {
        return {
          success: false,
          message: error instanceof Error ? error.message : "Unknown error",
        };
      }
    }

    // Mock para browser
    await delay(1500);
    return {
      success: true,
      message: "Connection successful (mock)",
    };
  }

  /**
   * Invalida o cache do adapter (chamar após atualizar provider)
   */
  static async invalidateAdapter(providerId: string): Promise<void> {
    if (isDesktopAi() && window.desktopAPI?.ai) {
      await window.desktopAPI.ai.invalidateAdapter(providerId);
    }
  }
}

// Factory para criar instância do serviço
export function createAIService(config: AIServiceConfig): AIService {
  return new AIService(config);
}

// ============================================
// Hook de conveniência para usar o serviço
// ============================================

export function useAIService(config: AIServiceConfig) {
  const service = new AIService(config);

  return {
    generatePlan: () => service.generatePlan(),
    generateCode: () => service.generateCode(),
    applyChanges: (options?: { createBackup?: boolean; dryRun?: boolean }) =>
      service.applyChanges(options),
    validateProvider: () => AIService.validateProvider(config.provider),
    testConnection: () => AIService.testConnection(config.provider.id),
  };
}
