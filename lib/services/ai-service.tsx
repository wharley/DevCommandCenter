"use client";

import type {
  Provider,
  Comb,
} from "@/lib/database/types";

export interface AIServiceConfig {
  provider: Provider;
  comb: Comb;
  projectContext?: {
    files: string[];
    fileContents?: Record<string, string>;
  };
}

export interface AIResponse<T = any> {
  success: boolean;
  data?: T;
  error?: string;
  metadata?: {
    tokensUsed?: number;
    durationMs?: number;
    model?: string;
  };
}

/**
 * Serviço de IA que centraliza chamadas a LLMs (via backend Tauri ou direto)
 */
export class AIService {
  private config: AIServiceConfig;

  constructor(config: AIServiceConfig) {
    this.config = config;
  }

  async generatePlan(planFeedback?: string): Promise<AIResponse> {
    // Legacy - placeholder
    return { success: false, error: "AI Service refactoring in progress" };
  }

  async generateCode(options?: {
    plan?: any;
    feedback?: string;
  }): Promise<AIResponse> {
    // Legacy - placeholder
    return { success: false, error: "AI Service refactoring in progress" };
  }

  async applyChanges(code: any): Promise<AIResponse> {
    // Legacy - placeholder
    return { success: false, error: "AI Service refactoring in progress" };
  }
}
