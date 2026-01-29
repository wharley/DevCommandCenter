/**
 * Claude Code CLI Adapter
 *
 * Integração com o Claude Code CLI (https://claude.ai/code)
 * Usa o CLI instalado localmente para executar comandos
 */

import { spawn, execSync, type ChildProcess } from "node:child_process";
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
  ProgressCallback,
} from "../types";

// Timeout padrão de 10 minutos (em ms)
const DEFAULT_TIMEOUT_MS = 10 * 60 * 1000;
// Timeout de inatividade (sem receber chunks) - 2 minutos
const INACTIVITY_TIMEOUT_MS = 2 * 60 * 1000;

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
      errors.push(
        "CLI path is required for Claude Code. Please configure the path to the 'claude' executable.",
      );
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

  async generatePlan(
    config: AdapterConfig,
    onProgress?: ProgressCallback,
  ): Promise<AIResponse<MissionPlan>> {
    const startTime = Date.now();
    const validation = this.validate();

    if (!validation.valid) {
      return {
        success: false,
        error: validation.errors.join("; "),
      };
    }

    try {
      onProgress?.("Preparando prompt para geração do plano...");
      const prompt = this.buildPlanPrompt(config);

      onProgress?.("Conectando ao Claude CLI...");
      const response = await this.executeClaudeCommand(
        prompt,
        config.projectContext.projectPath,
        onProgress,
      );

      onProgress?.("Processando resposta...");
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

      onProgress?.("Plano gerado com sucesso!");
      return {
        success: true,
        data: plan,
        metadata: {
          durationMs: Date.now() - startTime,
          provider: this.name,
          model: (this.provider.config?.model as string) || "claude-code",
        },
      };
    } catch (error) {
      return {
        success: false,
        error:
          error instanceof Error
            ? error.message
            : "Unknown error generating plan",
        metadata: {
          durationMs: Date.now() - startTime,
          provider: this.name,
        },
      };
    }
  }

  async generateCode(
    config: AdapterConfig,
    onProgress?: ProgressCallback,
  ): Promise<AIResponse<GeneratedCode>> {
    const startTime = Date.now();
    const validation = this.validate();

    if (!validation.valid) {
      return {
        success: false,
        error: validation.errors.join("; "),
      };
    }

    try {
      onProgress?.("Preparando prompt para geração de código...");
      const prompt = this.buildCodePrompt(config);

      onProgress?.("Conectando ao Claude CLI...");
      const response = await this.executeClaudeCommand(
        prompt,
        config.projectContext.projectPath,
        onProgress,
      );

      onProgress?.("Processando resposta...");
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

      onProgress?.("Código gerado com sucesso!");
      return {
        success: true,
        data: code,
        metadata: {
          durationMs: Date.now() - startTime,
          provider: this.name,
          model: (this.provider.config?.model as string) || "claude-code",
        },
      };
    } catch (error) {
      return {
        success: false,
        error:
          error instanceof Error
            ? error.message
            : "Unknown error generating code",
        metadata: {
          durationMs: Date.now() - startTime,
          provider: this.name,
        },
      };
    }
  }

  /**
   * Normaliza o modelo para o CLI: Sonnet 4.5 / Opus 4.5 usam aliases para ser à prova de novas datas.
   */
  private normalizeModelForCli(model: string): string {
    if (/^claude-sonnet-4-5-/.test(model)) return "sonnet";
    if (/^claude-opus-4-5-/.test(model)) return "opus";
    if (/^claude-haiku-4-5-/.test(model)) return "haiku";
    return model;
  }

  /**
   * Executa o comando Claude CLI com suporte a streaming e timeout inteligente
   *
   * Conforme documentação oficial (code.claude.com/docs/en/common-workflows):
   * - Usa -p <prompt> para modo não-interativo
   * - Usa --output-format json para facilitar parsing
   * - Streaming de resposta com feedback de progresso
   * - Timeout com heartbeat (reseta a cada chunk recebido)
   * - Timeout configurável via provider config
   */
  private executeClaudeCommand(
    prompt: string,
    cwd: string,
    onProgress?: ProgressCallback,
  ): Promise<string> {
    return new Promise((resolve, reject) => {
      const cliPath = this.provider.cliPath!;

      // Obtém timeout configurável do provider ou usa o padrão
      const maxTimeout =
        (this.provider.config?.timeout as number) || DEFAULT_TIMEOUT_MS;
      const inactivityTimeout = Math.min(INACTIVITY_TIMEOUT_MS, maxTimeout / 2);

      // Argumentos conforme documentação oficial do Claude CLI (headless):
      // -p <prompt>: Especifica o prompt (obrigatório para modo não-interativo)
      // --output-format json: Formato de saída estruturado
      // --allowedTools: Auto-aprova ferramentas sem pedir confirmação (evita bloqueio em "Do you want to make this edit?")
      const allowedTools =
        (this.provider.config?.allowedTools as string) || "Read,Edit,Bash";
      const args = [
        "-p",
        prompt, // Prompt como argumento de -p
        "--output-format",
        "json", // JSON para facilitar parse da resposta
        "--allowedTools",
        allowedTools,
      ];

      // Adiciona modelo: usa alias do CLI (sonnet/opus) para 4.5 para ser à prova de novas datas
      const rawModel = this.provider.config?.model as string;
      const model = rawModel ? this.normalizeModelForCli(rawModel) : undefined;
      if (model) {
        args.unshift("--model", model);
      }

      let child: ChildProcess;
      let stdout = "";
      let stderr = "";
      let inactivityTimeoutId: NodeJS.Timeout | undefined;
      let maxTimeoutId: NodeJS.Timeout;
      let chunksReceived = 0;
      let anyOutputReceived = false;
      let isResolved = false;
      let inactivityTimerStarted = false;

      const cleanup = () => {
        clearTimeout(inactivityTimeoutId);
        clearTimeout(maxTimeoutId);
      };

      const handleResolve = (value: string) => {
        if (isResolved) return;
        isResolved = true;
        cleanup();
        resolve(value);
      };

      const handleReject = (error: Error) => {
        if (isResolved) return;
        isResolved = true;
        cleanup();
        try {
          child?.kill();
        } catch {
          // Ignora erros ao matar o processo
        }
        reject(error);
      };

      // Timeout de inatividade só após o primeiro chunk (período de graça para TTFT).
      // Reseta a cada chunk recebido.
      const resetInactivityTimeout = () => {
        clearTimeout(inactivityTimeoutId);
        inactivityTimeoutId = setTimeout(() => {
          handleReject(
            new Error(
              `Claude CLI inativo por ${inactivityTimeout / 60000} minutos. ` +
                `Chunks recebidos: ${chunksReceived}. ` +
                `Considere verificar a conexão ou aumentar o timeout.`,
            ),
          );
        }, inactivityTimeout);
      };

      try {
        child = spawn(cliPath, args, {
          cwd,
          env: {
            ...process.env,
            // Passa API key se configurada (para casos onde não usa login)
            ANTHROPIC_API_KEY:
              this.provider.apiKey || process.env.ANTHROPIC_API_KEY,
          },
          shell: platform() === "win32",
          stdio: ["ignore", "pipe", "pipe"],
        });

        child.stdout?.on("data", (data) => {
          const chunk = data.toString();
          stdout += chunk;
          chunksReceived++;
          anyOutputReceived = true;
          if (!inactivityTimerStarted) inactivityTimerStarted = true;
          // Inicia/reseta o timeout de inatividade (só conta após o primeiro chunk)
          resetInactivityTimeout();

          // Notifica progresso a cada 5 chunks ou se o chunk tiver conteúdo significativo
          if (chunksReceived % 5 === 0 || chunk.length > 100) {
            const sizeKB = (stdout.length / 1024).toFixed(1);
            onProgress?.(
              `Recebendo resposta... (${sizeKB} KB, ${chunksReceived} chunks)`,
            );
          }
        });

        child.stderr?.on("data", (data) => {
          const chunk = data.toString();
          stderr += chunk;
          anyOutputReceived = true;
          // Stderr reseta o timeout só se já recebemos stdout (timer já iniciado)
          if (inactivityTimerStarted) resetInactivityTimeout();
        });

        child.on("close", (code) => {
          if (code === 0) {
            onProgress?.(
              `Resposta completa recebida (${(stdout.length / 1024).toFixed(1)} KB)`,
            );
            // Com --output-format json o CLI retorna { result, session_id, ... }; extrair result para parse do plano/código
            let payload = stdout;
            try {
              const wrapper = JSON.parse(stdout) as { result?: string };
              if (typeof wrapper?.result === "string") {
                payload = wrapper.result;
              }
            } catch {
              // stdout não é JSON wrapper; usar como está (fallback para texto puro)
            }
            handleResolve(payload);
          } else {
            // Inclui stderr na mensagem de erro para facilitar debug
            const errorDetails = [
              `Claude CLI exited with code ${code}.`,
              `Chunks recebidos: ${chunksReceived}.`,
              stderr ? `Stderr: ${stderr}` : "Sem mensagem de erro no stderr.",
            ].join(" ");
            handleReject(new Error(errorDetails));
          }
        });

        child.on("error", (error) => {
          handleReject(error);
        });

        // Timeout máximo absoluto (inatividade só após o primeiro chunk)
        maxTimeoutId = setTimeout(() => {
          const stderrSnippet =
            stderr.length > 0 ? ` Stderr: ${stderr.slice(-500).trim()}` : "";
          if (chunksReceived === 0) {
            if (anyOutputReceived) {
              handleReject(
                new Error(
                  "Claude CLI não enviou resposta (stdout) dentro do tempo máximo. Saída em stderr:" +
                    stderrSnippet,
                ),
              );
            } else {
              handleReject(
                new Error(
                  "Nenhum dado recebido do Claude CLI dentro do tempo máximo. " +
                    "O primeiro token pode demorar vários minutos em prompts longos. " +
                    "Verifique conexão, aumente o timeout do provider ou simplifique o prompt." +
                    stderrSnippet,
                ),
              );
            }
          } else {
            handleReject(
              new Error(
                `Claude CLI timeout máximo (${maxTimeout / 60000} minutos). ` +
                  `Chunks recebidos: ${chunksReceived}. ` +
                  `Resposta parcial: ${stdout.slice(0, 200)}...` +
                  stderrSnippet,
              ),
            );
          }
        }, maxTimeout);
      } catch (error) {
        handleReject(error instanceof Error ? error : new Error(String(error)));
      }
    });
  }
}

// Factory function
export function createClaudeCodeAdapter(provider: Provider): ClaudeCodeAdapter {
  return new ClaudeCodeAdapter(provider);
}
