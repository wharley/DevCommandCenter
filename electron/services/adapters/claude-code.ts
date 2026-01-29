/**
 * Claude Code CLI Adapter
 * 
 * Integração com o Claude Code CLI (https://claude.ai/code)
 * Usa o CLI instalado localmente para executar comandos
 */

import { spawn, execSync } from "node:child_process";
import { platform } from "node:os";
import * as fs from "node:fs";
import { BaseAdapter } from "./base";
import type {
  ValidationResult,
  AdapterConfig,
  AIResponse,
  MissionPlan,
  GeneratedCode,
  Provider,
} from "../types";

export class ClaudeCodeAdapter extends BaseAdapter {
  readonly name = "Claude Code";
  readonly type = "claude-code" as const;

  constructor(provider: Provider) {
    super(provider);
  }

  validate(): ValidationResult {
    const errors: string[] = [];
    const warnings: string[] = [];

    // Verifica se o CLI path está configurado
    if (!this.provider.cliPath) {
      errors.push("CLI path is required for Claude Code. Please configure the path to the 'claude' executable.");
    } else if (!fs.existsSync(this.provider.cliPath)) {
      errors.push(`CLI not found at: ${this.provider.cliPath}`);
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
      // Tenta rodar --version para verificar se o CLI funciona
      const result = execSync(`"${this.provider.cliPath}" --version`, {
        encoding: "utf-8",
        timeout: 10000,
      });

      return {
        success: true,
        message: `Claude Code CLI detected: ${result.trim()}`,
      };
    } catch (error) {
      return {
        success: false,
        message: `Failed to run Claude Code CLI: ${error instanceof Error ? error.message : "Unknown error"}`,
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
      const response = await this.executeClaudeCommand(prompt, config.projectContext.projectPath);

      const plan = this.parseJSONResponse<MissionPlan>(response);

      if (!plan) {
        return {
          success: false,
          error: "Failed to parse plan from Claude response",
          metadata: {
            durationMs: Date.now() - startTime,
            provider: this.name,
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
          model: this.provider.config?.model as string || "claude-code",
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
      const response = await this.executeClaudeCommand(prompt, config.projectContext.projectPath);

      const code = this.parseJSONResponse<GeneratedCode>(response);

      if (!code) {
        return {
          success: false,
          error: "Failed to parse code from Claude response",
          metadata: {
            durationMs: Date.now() - startTime,
            provider: this.name,
          },
        };
      }

      return {
        success: true,
        data: code,
        metadata: {
          durationMs: Date.now() - startTime,
          provider: this.name,
          model: this.provider.config?.model as string || "claude-code",
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
   * Executa o comando Claude CLI
   */
  private executeClaudeCommand(prompt: string, cwd: string): Promise<string> {
    return new Promise((resolve, reject) => {
      const cliPath = this.provider.cliPath!;
      
      // Flags para output JSON estruturado
      const args = [
        "--print",                    // Apenas imprime a resposta
        "--output-format", "text",    // Output em texto
        prompt,                       // O prompt
      ];

      // Adiciona modelo se configurado
      const model = this.provider.config?.model as string;
      if (model) {
        args.unshift("--model", model);
      }

      const child = spawn(cliPath, args, {
        cwd,
        env: {
          ...process.env,
          // Passa API key se configurada (para casos onde não usa login)
          ANTHROPIC_API_KEY: this.provider.apiKey || process.env.ANTHROPIC_API_KEY,
        },
        shell: platform() === "win32",
      });

      let stdout = "";
      let stderr = "";

      child.stdout?.on("data", (data) => {
        stdout += data.toString();
      });

      child.stderr?.on("data", (data) => {
        stderr += data.toString();
      });

      child.on("close", (code) => {
        if (code === 0) {
          resolve(stdout);
        } else {
          reject(new Error(stderr || `Claude CLI exited with code ${code}`));
        }
      });

      child.on("error", (error) => {
        reject(error);
      });

      // Timeout de 5 minutos
      setTimeout(() => {
        child.kill();
        reject(new Error("Claude CLI timeout (5 minutes)"));
      }, 5 * 60 * 1000);
    });
  }
}

// Factory function
export function createClaudeCodeAdapter(provider: Provider): ClaudeCodeAdapter {
  return new ClaudeCodeAdapter(provider);
}
