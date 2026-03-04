/**
 * Anthropic API Adapter
 * 
 * Integração direta com a API da Anthropic (Claude)
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

const ANTHROPIC_API_URL = "https://api.anthropic.com/v1/messages";
const DEFAULT_MODEL = "claude-3-5-sonnet-20241022";
const API_VERSION = "2023-06-01";

interface AnthropicMessage {
  role: "user" | "assistant";
  content: string;
}

interface AnthropicResponse {
  id: string;
  type: string;
  role: string;
  content: Array<{
    type: "text";
    text: string;
  }>;
  model: string;
  stop_reason: string;
  stop_sequence: string | null;
  usage: {
    input_tokens: number;
    output_tokens: number;
  };
}

export class AnthropicAdapter extends BaseAdapter {
  readonly name = "Anthropic API";
  readonly type = "anthropic" as const;

  constructor(provider: Provider) {
    super(provider);
  }

  validate(): ValidationResult {
    const errors: string[] = [];
    const warnings: string[] = [];

    // API Key é obrigatória
    if (!this.provider.apiKey) {
      errors.push("API key is required for Anthropic. Please configure your Anthropic API key.");
    } else if (!this.provider.apiKey.startsWith("sk-ant-")) {
      warnings.push("API key doesn't start with 'sk-ant-'. Make sure it's a valid Anthropic API key.");
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
      const response = await fetch(ANTHROPIC_API_URL, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "x-api-key": this.provider.apiKey!,
          "anthropic-version": API_VERSION,
        },
        body: JSON.stringify({
          model: this.getModel(),
          max_tokens: 10,
          messages: [{ role: "user", content: "Hello" }],
        }),
      });

      if (response.ok) {
        return {
          success: true,
          message: `Anthropic API connected successfully. Model: ${this.getModel()}`,
        };
      } else {
        const error = await response.json();
        return {
          success: false,
          message: `Anthropic API error: ${error.error?.message || response.statusText}`,
        };
      }
    } catch (error) {
      return {
        success: false,
        message: `Failed to connect to Anthropic API: ${error instanceof Error ? error.message : "Unknown error"}`,
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
      const systemPrompt = "You are an expert software engineer. Always respond with valid JSON only, no markdown or additional text.";
      
      const messages: AnthropicMessage[] = [
        {
          role: "user",
          content: prompt,
        },
      ];

      const response = await this.callAnthropic(messages, systemPrompt);
      const content = response.content[0]?.text;

      if (!content) {
        return {
          success: false,
          error: "Empty response from Anthropic",
          metadata: {
            durationMs: Date.now() - startTime,
            provider: this.name,
            tokensUsed: response.usage?.input_tokens + response.usage?.output_tokens,
          },
        };
      }

      const planResult = this.parseAndValidateMissionPlan(content);

      if (!planResult.success) {
        return {
          success: false,
          error: planResult.error,
          retryable: planResult.retryable,
          metadata: {
            durationMs: Date.now() - startTime,
            provider: this.name,
            tokensUsed: response.usage?.input_tokens + response.usage?.output_tokens,
          },
        };
      }

      const plan = planResult.data;
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
          tokensUsed: response.usage?.input_tokens + response.usage?.output_tokens,
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
      const systemPrompt = "You are an expert software engineer. Generate production-ready code. Always respond with valid JSON only, no markdown or additional text.";
      
      const messages: AnthropicMessage[] = [
        {
          role: "user",
          content: prompt,
        },
      ];

      const response = await this.callAnthropic(messages, systemPrompt, { max_tokens: 8000 });
      const content = response.content[0]?.text;

      if (!content) {
        return {
          success: false,
          error: "Empty response from Anthropic",
          metadata: {
            durationMs: Date.now() - startTime,
            provider: this.name,
            tokensUsed: response.usage?.input_tokens + response.usage?.output_tokens,
          },
        };
      }

      const codeResult = this.parseAndValidateGeneratedCode(content);

      if (!codeResult.success) {
        return {
          success: false,
          error: `Failed to parse code: ${codeResult.error}`,
          metadata: {
            durationMs: Date.now() - startTime,
            provider: this.name,
            tokensUsed: response.usage?.input_tokens + response.usage?.output_tokens,
          },
        };
      }

      const code = codeResult.data;
      return {
        success: true,
        data: code,
        metadata: {
          durationMs: Date.now() - startTime,
          provider: this.name,
          model: response.model,
          tokensUsed: response.usage?.input_tokens + response.usage?.output_tokens,
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
   * Chama a API da Anthropic
   */
  private async callAnthropic(
    messages: AnthropicMessage[],
    system: string,
    options: { max_tokens?: number; temperature?: number } = {}
  ): Promise<AnthropicResponse> {
    const response = await fetch(ANTHROPIC_API_URL, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "x-api-key": this.provider.apiKey!,
        "anthropic-version": API_VERSION,
      },
      body: JSON.stringify({
        model: this.getModel(),
        max_tokens: options.max_tokens || 4000,
        system,
        messages,
      }),
    });

    if (!response.ok) {
      const error = await response.json();
      throw new Error(error.error?.message || `Anthropic API error: ${response.statusText}`);
    }

    return response.json();
  }
}

// Factory function
export function createAnthropicAdapter(provider: Provider): AnthropicAdapter {
  return new AnthropicAdapter(provider);
}
