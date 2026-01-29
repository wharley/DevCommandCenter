/**
 * OpenAI API Adapter
 * 
 * Integração direta com a API da OpenAI (GPT-4, etc.)
 * Usa fetch para chamadas REST
 */

import { BaseAdapter } from "./base";
import type {
  ValidationResult,
  AdapterConfig,
  AIResponse,
  MissionPlan,
  GeneratedCode,
  Provider,
} from "../types";

const OPENAI_API_URL = "https://api.openai.com/v1/chat/completions";
const DEFAULT_MODEL = "gpt-4-turbo-preview";

interface OpenAIMessage {
  role: "system" | "user" | "assistant";
  content: string;
}

interface OpenAIResponse {
  id: string;
  object: string;
  created: number;
  model: string;
  choices: Array<{
    index: number;
    message: {
      role: string;
      content: string;
    };
    finish_reason: string;
  }>;
  usage: {
    prompt_tokens: number;
    completion_tokens: number;
    total_tokens: number;
  };
}

export class OpenAIAdapter extends BaseAdapter {
  readonly name = "OpenAI API";
  readonly type = "openai" as const;

  constructor(provider: Provider) {
    super(provider);
  }

  validate(): ValidationResult {
    const errors: string[] = [];
    const warnings: string[] = [];

    // API Key é obrigatória
    if (!this.provider.apiKey) {
      errors.push("API key is required for OpenAI. Please configure your OpenAI API key.");
    } else if (!this.provider.apiKey.startsWith("sk-")) {
      warnings.push("API key doesn't start with 'sk-'. Make sure it's a valid OpenAI API key.");
    }

    return {
      valid: errors.length === 0,
      errors,
      warnings,
    };
  }

  async testConnection(): Promise<{ success: boolean; message: string }> {
    const validation = this.validate();
    if (!validation.valid) {
      return {
        success: false,
        message: validation.errors.join("; "),
      };
    }

    try {
      // Faz uma chamada simples para verificar a API key
      const response = await fetch(OPENAI_API_URL, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${this.provider.apiKey}`,
        },
        body: JSON.stringify({
          model: this.getModel(),
          messages: [{ role: "user", content: "Hello" }],
          max_tokens: 5,
        }),
      });

      if (response.ok) {
        return {
          success: true,
          message: `OpenAI API connected successfully. Model: ${this.getModel()}`,
        };
      } else {
        const error = await response.json();
        return {
          success: false,
          message: `OpenAI API error: ${error.error?.message || response.statusText}`,
        };
      }
    } catch (error) {
      return {
        success: false,
        message: `Failed to connect to OpenAI API: ${error instanceof Error ? error.message : "Unknown error"}`,
      };
    }
  }

  async generatePlan(config: AdapterConfig): Promise<AIResponse<MissionPlan>> {
    const startTime = Date.now();
    const validation = this.validate();

    if (!validation.valid) {
      return {
        success: false,
        error: validation.errors.join("; "),
      };
    }

    try {
      const prompt = this.buildPlanPrompt(config);
      const messages: OpenAIMessage[] = [
        {
          role: "system",
          content: "You are an expert software engineer. Always respond with valid JSON only, no markdown or additional text.",
        },
        {
          role: "user",
          content: prompt,
        },
      ];

      const response = await this.callOpenAI(messages);
      const content = response.choices[0]?.message?.content;

      if (!content) {
        return {
          success: false,
          error: "Empty response from OpenAI",
          metadata: {
            durationMs: Date.now() - startTime,
            provider: this.name,
            tokensUsed: response.usage?.total_tokens,
          },
        };
      }

      const plan = this.parseJSONResponse<MissionPlan>(content);

      if (!plan) {
        return {
          success: false,
          error: "Failed to parse plan from OpenAI response",
          metadata: {
            durationMs: Date.now() - startTime,
            provider: this.name,
            tokensUsed: response.usage?.total_tokens,
          },
        };
      }

      // Garante que os steps têm IDs únicos
      if (plan.steps) {
        plan.steps = plan.steps.map((step, index) => ({
          ...step,
          id: step.id || this.generateStepId(index),
        }));
      }

      return {
        success: true,
        data: plan,
        metadata: {
          durationMs: Date.now() - startTime,
          provider: this.name,
          model: response.model,
          tokensUsed: response.usage?.total_tokens,
        },
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : "Unknown error generating plan",
        metadata: {
          durationMs: Date.now() - startTime,
          provider: this.name,
        },
      };
    }
  }

  async generateCode(config: AdapterConfig): Promise<AIResponse<GeneratedCode>> {
    const startTime = Date.now();
    const validation = this.validate();

    if (!validation.valid) {
      return {
        success: false,
        error: validation.errors.join("; "),
      };
    }

    try {
      const prompt = this.buildCodePrompt(config);
      const messages: OpenAIMessage[] = [
        {
          role: "system",
          content: "You are an expert software engineer. Generate production-ready code. Always respond with valid JSON only, no markdown or additional text.",
        },
        {
          role: "user",
          content: prompt,
        },
      ];

      const response = await this.callOpenAI(messages, { max_tokens: 4000 });
      const content = response.choices[0]?.message?.content;

      if (!content) {
        return {
          success: false,
          error: "Empty response from OpenAI",
          metadata: {
            durationMs: Date.now() - startTime,
            provider: this.name,
            tokensUsed: response.usage?.total_tokens,
          },
        };
      }

      const code = this.parseJSONResponse<GeneratedCode>(content);

      if (!code) {
        return {
          success: false,
          error: "Failed to parse code from OpenAI response",
          metadata: {
            durationMs: Date.now() - startTime,
            provider: this.name,
            tokensUsed: response.usage?.total_tokens,
          },
        };
      }

      return {
        success: true,
        data: code,
        metadata: {
          durationMs: Date.now() - startTime,
          provider: this.name,
          model: response.model,
          tokensUsed: response.usage?.total_tokens,
        },
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : "Unknown error generating code",
        metadata: {
          durationMs: Date.now() - startTime,
          provider: this.name,
        },
      };
    }
  }

  /**
   * Obtém o modelo configurado ou usa o padrão
   */
  private getModel(): string {
    return (this.provider.config?.model as string) || DEFAULT_MODEL;
  }

  /**
   * Chama a API da OpenAI
   */
  private async callOpenAI(
    messages: OpenAIMessage[],
    options: { max_tokens?: number; temperature?: number } = {}
  ): Promise<OpenAIResponse> {
    const response = await fetch(OPENAI_API_URL, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${this.provider.apiKey}`,
      },
      body: JSON.stringify({
        model: this.getModel(),
        messages,
        max_tokens: options.max_tokens || 2000,
        temperature: options.temperature ?? 0.7,
      }),
    });

    if (!response.ok) {
      const error = await response.json();
      throw new Error(error.error?.message || `OpenAI API error: ${response.statusText}`);
    }

    return response.json();
  }
}

// Factory function
export function createOpenAIAdapter(provider: Provider): OpenAIAdapter {
  return new OpenAIAdapter(provider);
}
