// Dev Command Center - AI Service
// Serviço para comunicação com providers de IA
// Usa IPC para comunicar com o backend Electron ou mock para browser

import type {
  Provider,
  Mission,
  MissionPlan,
  GeneratedCode,
  PlanStep,
  CodeSuggestion,
} from '@/lib/database/types';

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
// Detecta ambiente Electron
// ============================================

const isElectron = () => typeof window !== "undefined" && !!window.electronAPI?.ai;

// ============================================
// Simulação de respostas (para browser/dev)
// ============================================

const delay = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

const generateMockPlan = (mission: Mission): MissionPlan => {
  const baseSteps: Omit<PlanStep, 'id'>[] = [
    {
      order: 1,
      title: 'Analyze current implementation',
      description: `Review existing code related to: ${mission.title}`,
      files: ['src/components/', 'src/lib/'],
      status: 'pending',
    },
    {
      order: 2,
      title: 'Design solution architecture',
      description: 'Create technical design and identify affected components',
      status: 'pending',
    },
    {
      order: 3,
      title: 'Implement core changes',
      description: mission.description.slice(0, 100),
      files: ['src/'],
      status: 'pending',
    },
    {
      order: 4,
      title: 'Update related components',
      description: 'Modify dependent components and update imports',
      status: 'pending',
    },
    {
      order: 5,
      title: 'Add tests and documentation',
      description: 'Write unit tests and update documentation',
      files: ['__tests__/', 'docs/'],
      status: 'pending',
    },
  ];

  return {
    summary: `Implementation plan for: ${mission.title}. This plan covers analysis, implementation, and testing phases.`,
    estimatedComplexity: mission.description.length > 200 ? 'high' : mission.description.length > 100 ? 'medium' : 'low',
    steps: baseSteps.map((step, index) => ({
      ...step,
      id: `step-${Date.now()}-${index}`,
    })),
  };
};

const generateMockCode = (mission: Mission): GeneratedCode => {
  const suggestions: CodeSuggestion[] = [
    {
      path: 'src/components/NewFeature.tsx',
      action: 'create',
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
      path: 'src/lib/utils.ts',
      action: 'modify',
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
   * No Electron, usa IPC para chamar o backend
   * No browser, usa mock
   */
  async generatePlan(): Promise<AIResponse<MissionPlan>> {
    // Se estamos no Electron, usa o backend real
    if (isElectron() && window.electronAPI?.ai) {
      try {
        const result = await window.electronAPI.ai.generatePlan(this.config.mission.id);
        return result as AIResponse<MissionPlan>;
      } catch (error) {
        return {
          success: false,
          error: error instanceof Error ? error.message : 'Unknown error calling AI service',
        };
      }
    }

    // Fallback para mock (browser ou dev sem Electron)
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
          model: this.config.provider.config?.model as string || 'mock',
          provider: 'Mock (Browser)',
        },
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error generating plan',
      };
    }
  }

  /**
   * Gera sugestões de código baseadas no plano
   * No Electron, usa IPC para chamar o backend
   * No browser, usa mock
   */
  async generateCode(): Promise<AIResponse<GeneratedCode>> {
    // Se estamos no Electron, usa o backend real
    if (isElectron() && window.electronAPI?.ai) {
      try {
        const result = await window.electronAPI.ai.generateCode(this.config.mission.id);
        return result as AIResponse<GeneratedCode>;
      } catch (error) {
        return {
          success: false,
          error: error instanceof Error ? error.message : 'Unknown error calling AI service',
        };
      }
    }

    // Fallback para mock (browser ou dev sem Electron)
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
          model: this.config.provider.config?.model as string || 'mock',
          provider: 'Mock (Browser)',
        },
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error generating code',
      };
    }
  }

  /**
   * Aplica as mudanças ao projeto
   * Só funciona no Electron
   */
  async applyChanges(options?: { createBackup?: boolean; dryRun?: boolean }): Promise<ApplyChangesResult> {
    if (isElectron() && window.electronAPI?.ai) {
      try {
        return await window.electronAPI.ai.applyChanges(this.config.mission.id, options);
      } catch (error) {
        return {
          success: false,
          appliedFiles: [],
          failedFiles: [{ path: '', error: error instanceof Error ? error.message : 'Unknown error' }],
        };
      }
    }

    // Mock para browser
    await delay(1000);
    return {
      success: true,
      appliedFiles: ['src/components/NewFeature.tsx', 'src/lib/utils.ts'],
      failedFiles: [],
      backupPath: '/mock/backup/path',
    };
  }

  /**
   * Verifica se o provider está configurado corretamente
   */
  static validateProvider(provider: Provider): ValidationResult {
    const errors: string[] = [];
    const warnings: string[] = [];
    
    if (!provider.name) {
      errors.push('Provider name is required');
    }
    
    if (provider.type === 'openai' || provider.type === 'anthropic') {
      if (!provider.apiKey) {
        errors.push('API key is required for this provider');
      }
    }
    
    if (provider.type === 'claude-code' || provider.type === 'codex') {
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
  static async validateProviderAsync(provider: Provider): Promise<ValidationResult> {
    if (isElectron() && window.electronAPI?.ai) {
      try {
        return await window.electronAPI.ai.validateProvider(provider);
      } catch (error) {
        return {
          valid: false,
          errors: [error instanceof Error ? error.message : 'Unknown error'],
        };
      }
    }
    return AIService.validateProvider(provider);
  }

  /**
   * Testa a conexão com o provider
   */
  static async testConnection(providerId: string): Promise<{ success: boolean; message: string }> {
    if (isElectron() && window.electronAPI?.ai) {
      try {
        return await window.electronAPI.ai.testConnection(providerId);
      } catch (error) {
        return {
          success: false,
          message: error instanceof Error ? error.message : 'Unknown error',
        };
      }
    }

    // Mock para browser
    await delay(1500);
    return {
      success: true,
      message: 'Connection successful (mock)',
    };
  }

  /**
   * Invalida o cache do adapter (chamar após atualizar provider)
   */
  static async invalidateAdapter(providerId: string): Promise<void> {
    if (isElectron() && window.electronAPI?.ai) {
      await window.electronAPI.ai.invalidateAdapter(providerId);
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
