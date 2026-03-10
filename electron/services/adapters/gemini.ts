/**
 * Gemini CLI Adapter
 *
 * Integração com o Gemini CLI (https://github.com/google-gemini/gemini-cli)
 * Usa o CLI instalado localmente para executar comandos em modo headless.
 * Workspace: sempre passar cwd no spawn para o diretório do projeto; se o CLI
 * ganhar flags tipo --trust/--workspace no futuro, adicionar aqui para evitar
 * problema de permissão como no Cursor.
 */

import { execSync, type ChildProcess } from "node:child_process";
import { platform } from "node:os";
import * as fs from "node:fs";
import * as path from "node:path";
import { BaseAdapter } from "./base";
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

const DEFAULT_TIMEOUT_MS = 10 * 60 * 1000;
const INACTIVITY_TIMEOUT_MS = 2 * 60 * 1000;

/**
 * Resolve o caminho do binário Gemini CLI.
 * Se provider.cliPath estiver definido, usa esse; senão tenta "gemini" no PATH.
 */
function resolveCliPath(provider: Provider): string | null {
  if (provider.cliPath && fs.existsSync(provider.cliPath)) {
    return provider.cliPath;
  }
  const name = platform() === "win32" ? "gemini.cmd" : "gemini";
  const pathEnv = process.env.PATH || "";
  const sep = platform() === "win32" ? ";" : ":";
  for (const dir of pathEnv.split(sep)) {
    const candidate = path.join(dir.trim(), name);
    if (fs.existsSync(candidate)) return candidate;
  }
  return null;
}

export class GeminiAdapter extends BaseAdapter {
  readonly name = "Gemini CLI";
  readonly type = "gemini" as const;

  constructor(provider: Provider) {
    super(provider);
  }

  validate(): ValidationResult {
    const errors: string[] = [];
    const warnings: string[] = [];

    const cliPath = resolveCliPath(this.provider);
    if (!cliPath) {
      errors.push(
        "Gemini CLI not found. Please install it (npm install -g @google/gemini-cli) or set the path to the 'gemini' executable in the provider settings.",
      );
    }

    if (!this.provider.apiKey && !process.env.GEMINI_API_KEY) {
      warnings.push(
        "No API key configured. Make sure GEMINI_API_KEY is set in your environment or configure it in the provider. You can also authenticate via the CLI (run 'gemini' in terminal and sign in with Google).",
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

    const cliPath = resolveCliPath(this.provider)!;
    const cliEnv = { ...process.env, PATH: getResolvedPathForNode() };
    try {
      execSync(`"${cliPath}" --help`, {
        encoding: "utf-8",
        timeout: 10000,
        env: cliEnv,
      });
      return {
        success: true,
        message: "Gemini CLI detected.",
      };
    } catch {
      try {
        execSync(`"${cliPath}" --version`, {
          encoding: "utf-8",
          timeout: 10000,
          env: cliEnv,
        });
        return {
          success: true,
          message: "Gemini CLI detected.",
        };
      } catch (err) {
        return {
          success: false,
          message: `Failed to run Gemini CLI: ${
            err instanceof Error ? err.message : "Unknown error"
          }. Make sure you are authenticated (run 'gemini' in terminal if needed).`,
        };
      }
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

      onProgress?.("Conectando ao Gemini CLI...");

      const progressTimer = this.startProgressFeedback(onProgress, "plan");

      const response = await this.executeGeminiCommand(
        prompt,
        config.projectContext.projectPath,
        onProgress,
      );

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
          model: (this.provider.config?.model as string) || "gemini",
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

      onProgress?.("Conectando ao Gemini CLI...");

      const progressTimer = this.startProgressFeedback(onProgress, "code");

      const response = await this.executeGeminiCommand(
        prompt,
        config.projectContext.projectPath,
        onProgress,
      );

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
          model: (this.provider.config?.model as string) || "gemini",
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
   * Executa o comando Gemini CLI em modo headless.
   * cwd é obrigatório para o CLI rodar no workspace correto (evitar problema de
   * permissão como no Cursor). Se o Gemini CLI ganhar flags --trust/--workspace,
   * adicionar aqui.
   */
  private executeGeminiCommand(
    prompt: string,
    cwd: string,
    onProgress?: ProgressCallback,
  ): Promise<string> {
    return new Promise((resolve, reject) => {
      const cliPath = resolveCliPath(this.provider)!;

      const maxTimeout =
        (this.provider.config?.timeout as number) || DEFAULT_TIMEOUT_MS;
      const inactivityTimeout = Math.min(INACTIVITY_TIMEOUT_MS, maxTimeout / 2);

      const model = this.provider.config?.model as string | undefined;
      const args = [
        "--output-format",
        "json",
        ...(model && model.trim() ? ["--model", model] : []),
      ];

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
          // ignore
        }
        reject(error);
      };

      const resetInactivityTimeout = () => {
        clearTimeout(inactivityTimeoutId);
        inactivityTimeoutId = setTimeout(() => {
          handleReject(
            new Error(
              `Gemini CLI inativo por ${inactivityTimeout / 60000} minutos. ` +
                `Chunks recebidos: ${chunksReceived}.`,
            ),
          );
        }, inactivityTimeout);
      };

      try {
        const env = {
          ...process.env,
          GEMINI_API_KEY:
            this.provider.apiKey || process.env.GEMINI_API_KEY,
        };
        child = spawnCliWithLoginShell(cliPath, args, {
          cwd,
          env,
          shell: platform() === "win32",
          stdio: ["pipe", "pipe", "pipe"],
        });

        if (child.stdin) {
          child.stdin.write(prompt);
          child.stdin.end();
        } else {
          handleReject(new Error("Failed to write to Gemini CLI stdin"));
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
              `Recebendo resposta... (${sizeKB} KB, ${chunksReceived} chunks)`,
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
              `Resposta completa recebida (${(stdout.length / 1024).toFixed(1)} KB)`,
            );
            try {
              const parsed = JSON.parse(stdout) as {
                response?: string;
                error?: { message?: string; type?: string };
              };
              if (parsed.error) {
                handleReject(
                  new Error(
                    parsed.error.message ||
                      String(parsed.error.type || "Gemini CLI error"),
                  ),
                );
                return;
              }
              const responseText =
                typeof parsed.response === "string"
                  ? parsed.response
                  : stdout;
              handleResolve(responseText);
            } catch {
              handleResolve(stdout);
            }
          } else {
            handleReject(
              new Error(stderr || `Gemini CLI exited with code ${code}`),
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
                "Nenhum dado recebido do Gemini CLI dentro do tempo máximo. " +
                  "Verifique autenticação (rode 'gemini' no terminal se necessário) e aumente o timeout se precisar.",
              ),
            );
          } else {
            handleReject(
              new Error(
                `Gemini CLI timeout máximo (${maxTimeout / 60000} minutos). ` +
                  `Chunks recebidos: ${chunksReceived}.`,
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

export function createGeminiAdapter(provider: Provider): GeminiAdapter {
  return new GeminiAdapter(provider);
}
