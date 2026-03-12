/**
 * Codex CLI Adapter
 *
 * Integração com o OpenAI Codex CLI
 * Usa o CLI instalado localmente para executar comandos
 */

import { spawn, execSync, type ChildProcess } from "node:child_process";
import { platform } from "node:os";
import * as fs from "node:fs";
import { BaseAdapter } from "./base";
import { getPermissionFlagsForAdapter } from "./permission-modes";
import { spawnCliWithLoginShell, getResolvedPathForNode } from "../shell-path";
import type {
  ValidationResult,
  AdapterConfig,
  AIResponse,
  MissionPlan,
  GeneratedCode,
  Provider,
  ProgressCallback,
} from "../types";
import type { PermissionMode } from "../../../lib/database/types";

// Timeout padrão de 10 minutos (em ms)
const DEFAULT_TIMEOUT_MS = 10 * 60 * 1000;
// Timeout de inatividade (sem receber chunks) - 2 minutos
const INACTIVITY_TIMEOUT_MS = 2 * 60 * 1000;

export class CodexAdapter extends BaseAdapter {
  readonly name = "OpenAI Codex";
  readonly type = "codex" as const;

  constructor(provider: Provider) {
    super(provider);
  }

  validate(): ValidationResult {
    const errors: string[] = [];
    const warnings: string[] = [];

    // Verifica se o CLI path está configurado
    if (!this.provider.cliPath) {
      errors.push(
        "CLI path is required for Codex. Please configure the path to the 'codex' executable."
      );
    } else if (!fs.existsSync(this.provider.cliPath)) {
      errors.push(`CLI not found at: ${this.provider.cliPath}`);
    }

    // API Key é opcional se já estiver configurada no ambiente
    if (!this.provider.apiKey && !process.env.OPENAI_API_KEY) {
      warnings.push(
        "No API key configured. Make sure OPENAI_API_KEY is set in your environment."
      );
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
      // Tenta rodar --version para verificar se o CLI funciona (env.PATH para app aberto pelo Finder; stdio + -c check_for_update_on_startup=false evitam bloqueio no prompt de update)
      const result = execSync(
        `"${this.provider.cliPath}" -c check_for_update_on_startup=false --version`,
        {
          encoding: "utf-8",
          timeout: 10000,
          env: { ...process.env, PATH: getResolvedPathForNode() },
          stdio: ["ignore", "pipe", "pipe"],
        }
      );

      return {
        success: true,
        message: `Codex CLI detected: ${result.trim()}`,
      };
    } catch (error) {
      return {
        success: false,
        message: `Failed to run Codex CLI: ${
          error instanceof Error ? error.message : "Unknown error"
        }`,
      };
    }
  }

  async generatePlan(
    config: AdapterConfig,
    onProgress?: ProgressCallback
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

      onProgress?.("Conectando ao Codex CLI...");

      // Start timer-based progress feedback for better UX
      const progressTimer = this.startProgressFeedback(onProgress, "plan");

      const response = await this.executeCodexCommand(
        prompt,
        config.projectContext.projectPath,
        onProgress
      );

      // Clear timer-based progress
      if (progressTimer) clearInterval(progressTimer);

      onProgress?.("Processando resposta...");
      const planResult = this.parseAndValidateMissionPlan(response);

      if (!planResult.success) {
        return {
          success: false,
          error: planResult.error,
          retryable: planResult.retryable,
          metadata: {
            durationMs: Date.now() - startTime,
            provider: this.name,
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

      onProgress?.("Plano gerado com sucesso!");
      return {
        success: true,
        data: plan,
        pendingCommands: planResult.pendingCommands,
        metadata: {
          durationMs: Date.now() - startTime,
          provider: this.name,
          model: (this.provider.config?.model as string) || "codex",
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
    onProgress?: ProgressCallback
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

      onProgress?.("Conectando ao Codex CLI...");

      // Start timer-based progress feedback for better UX
      const progressTimer = this.startProgressFeedback(onProgress, "code");

      const response = await this.executeCodexCommand(
        prompt,
        config.projectContext.projectPath,
        onProgress
      );

      // Clear timer-based progress
      if (progressTimer) clearInterval(progressTimer);

      onProgress?.("Processando resposta...");
      const codeResult = this.parseAndValidateGeneratedCode(response);

      if (!codeResult.success) {
        const snippet = response.slice(0, 600).trim();
        return {
          success: false,
          error: `Failed to parse code: ${
            codeResult.error
          }. Raw snippet: ${snippet}${response.length > 600 ? "..." : ""}`,
          metadata: {
            durationMs: Date.now() - startTime,
            provider: this.name,
          },
        };
      }

      const code = codeResult.data;
      onProgress?.("Código gerado com sucesso!");
      return {
        success: true,
        data: code,
        pendingCommands: codeResult.pendingCommands,
        metadata: {
          durationMs: Date.now() - startTime,
          provider: this.name,
          model: (this.provider.config?.model as string) || "codex",
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
   * Executa o comando Codex CLI com suporte a streaming e timeout inteligente
   */
  private executeCodexCommand(
    prompt: string,
    cwd: string,
    onProgress?: ProgressCallback
  ): Promise<string> {
    return new Promise((resolve, reject) => {
      const cliPath = this.provider.cliPath!;

      // Obtém timeout configurável do provider ou usa o padrão
      const maxTimeout =
        (this.provider.config?.timeout as number) || DEFAULT_TIMEOUT_MS;
      const inactivityTimeout = Math.min(INACTIVITY_TIMEOUT_MS, maxTimeout / 2);

      // Args para o Codex CLI (codex exec = modo não-interativo para automação)
      // -c check_for_update_on_startup=false: não exibir prompt de atualização
      const permissionMode = (this.provider.config?.permissionMode ?? "acceptEdits") as PermissionMode;
      const permissionArgs = getPermissionFlagsForAdapter("codex", permissionMode);
      // Default: acceptEdits (--full-auto) when no flags from registry
      const hasPermissionFlags = permissionArgs.length > 0;
      const args = [
        "exec",
        "--cd",
        cwd,
        ...(hasPermissionFlags ? permissionArgs : ["--full-auto"]),
        "-c",
        "check_for_update_on_startup=false",
        "--skip-git-repo-check",
      ];

      const model = this.provider.config?.model as string;
      if (model) {
        args.push("--model", model);
      }

      args.push("-"); // prompt via stdin

      let child: ChildProcess;
      let stdout = "";
      let stderr = "";
      let inactivityTimeoutId: NodeJS.Timeout | undefined;
      let maxTimeoutId: NodeJS.Timeout;
      let chunksReceived = 0;
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
              `Codex CLI inativo por ${inactivityTimeout / 60000} minutos. ` +
                `Chunks recebidos: ${chunksReceived}.`
            )
          );
        }, inactivityTimeout);
      };

      try {
        const env = {
          ...process.env,
          OPENAI_API_KEY: this.provider.apiKey || process.env.OPENAI_API_KEY,
          ...(this.provider.apiKey && {
            CODEX_API_KEY: this.provider.apiKey,
          }),
        };
        child = spawnCliWithLoginShell(cliPath, args, {
          cwd,
          env,
          shell: platform() === "win32",
          stdio: ["pipe", "pipe", "pipe"],
        });

        // Envia o prompt via stdin
        if (child.stdin) {
          child.stdin.write(prompt);
          child.stdin.end();
        } else {
          handleReject(new Error("Failed to write to Codex CLI stdin"));
          return;
        }

        child.stdout?.on("data", (data) => {
          const chunk = data.toString();
          stdout += chunk;
          chunksReceived++;
          if (!inactivityTimerStarted) inactivityTimerStarted = true;
          resetInactivityTimeout();

          if (chunksReceived % 5 === 0 || chunk.length > 100) {
            const sizeKB = (stdout.length / 1024).toFixed(1);
            onProgress?.(
              `Recebendo resposta... (${sizeKB} KB, ${chunksReceived} chunks)`
            );
          }
        });

        child.stderr?.on("data", (data) => {
          stderr += data.toString();
          if (inactivityTimerStarted) resetInactivityTimeout();
        });

        child.on("close", (code) => {
          if (code === 0) {
            onProgress?.(
              `Resposta completa recebida (${(stdout.length / 1024).toFixed(
                1
              )} KB)`
            );
            handleResolve(stdout);
          } else {
            handleReject(
              new Error(stderr || `Codex CLI exited with code ${code}`)
            );
          }
        });

        child.on("error", (error) => {
          handleReject(error);
        });

        maxTimeoutId = setTimeout(() => {
          if (chunksReceived === 0) {
            handleReject(
              new Error(
                "Nenhum dado recebido do Codex CLI dentro do tempo máximo. " +
                  "O primeiro token pode demorar vários minutos em prompts longos. " +
                  "Verifique conexão, aumente o timeout do provider ou simplifique o prompt."
              )
            );
          } else {
            handleReject(
              new Error(
                `Codex CLI timeout máximo (${maxTimeout / 60000} minutos). ` +
                  `Chunks recebidos: ${chunksReceived}.`
              )
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
export function createCodexAdapter(provider: Provider): CodexAdapter {
  return new CodexAdapter(provider);
}
