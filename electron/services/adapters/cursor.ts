/**
 * Cursor Agent CLI Adapter
 *
 * Integração com o Cursor Agent CLI (https://cursor.com/docs/cli)
 * Usa o CLI instalado localmente (comando `agent`) para executar chat/tarefas
 * Login é feito no terminal; o app apenas invoca o binário já autenticado.
 */

import { spawn, execSync, type ChildProcess } from "node:child_process";
import { platform } from "node:os";
import * as fs from "node:fs";
import * as path from "node:path";
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

const DEFAULT_TIMEOUT_MS = 10 * 60 * 1000;
const INACTIVITY_TIMEOUT_MS = 2 * 60 * 1000;

const DEFAULT_CLI_NAME = "agent";

/**
 * Resolve o caminho do binário Cursor Agent CLI.
 * Se provider.cliPath estiver definido, usa esse; senão tenta "agent" no PATH.
 */
function resolveCliPath(provider: Provider): string | null {
  if (provider.cliPath && fs.existsSync(provider.cliPath)) {
    return provider.cliPath;
  }
  const name = platform() === "win32" ? "agent.exe" : "agent";
  const pathEnv = process.env.PATH || "";
  const sep = platform() === "win32" ? ";" : ":";
  for (const dir of pathEnv.split(sep)) {
    const candidate = path.join(dir.trim(), name);
    if (fs.existsSync(candidate)) return candidate;
  }
  return null;
}

export class CursorAdapter extends BaseAdapter {
  readonly name = "Cursor Agent CLI";
  readonly type = "cursor" as const;

  constructor(provider: Provider) {
    super(provider);
  }

  validate(): ValidationResult {
    const errors: string[] = [];
    const warnings: string[] = [];

    const cliPath = resolveCliPath(this.provider);
    if (!cliPath) {
      errors.push(
        "Cursor Agent CLI not found. Please install it (curl https://cursor.com/install -fsSL | bash) or set the path to the 'agent' executable in the provider settings.",
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
    try {
      // Cursor CLI pode não ter --version; tentar --help como verificação não-interativa
      execSync(`"${cliPath}" --help`, {
        encoding: "utf-8",
        timeout: 10000,
      });
      return {
        success: true,
        message: "Cursor Agent CLI detected.",
      };
    } catch {
      try {
        execSync(`"${cliPath}" --version`, {
          encoding: "utf-8",
          timeout: 10000,
        });
        return {
          success: true,
          message: "Cursor Agent CLI detected.",
        };
      } catch (err) {
        return {
          success: false,
          message: `Failed to run Cursor Agent CLI: ${err instanceof Error ? err.message : "Unknown error"}. Make sure you are logged in (run 'agent' in terminal if needed).`,
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

      onProgress?.("Conectando ao Cursor Agent CLI...");
      const response = await this.executeAgentCommand(
        "chat",
        prompt,
        config.projectContext.projectPath,
        onProgress,
      );

      onProgress?.("Processando resposta...");
      const plan = this.parseJSONResponse<MissionPlan>(response);

      if (!plan) {
        return {
          success: false,
          error: "Failed to parse plan from Cursor response",
          metadata: {
            durationMs: Date.now() - startTime,
            provider: this.name,
          },
        };
      }

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
          model: (this.provider.config?.model as string) || "cursor",
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

      onProgress?.("Conectando ao Cursor Agent CLI...");
      const response = await this.executeAgentCommand(
        "chat",
        prompt,
        config.projectContext.projectPath,
        onProgress,
      );

      onProgress?.("Processando resposta...");
      const code = this.parseJSONResponse<GeneratedCode>(response);

      if (!code) {
        const snippet = response.slice(0, 600).trim();
        return {
          success: false,
          error: `Failed to parse code from Cursor response. Raw response snippet: ${snippet}${response.length > 600 ? "..." : ""}`,
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
          model: (this.provider.config?.model as string) || "cursor",
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
   * Executa o comando Cursor Agent CLI (agent chat "<prompt>")
   * Documentação: https://cursor.com/docs/cli
   */
  private executeAgentCommand(
    subcommand: string,
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
      const modelArgs =
        model && model !== "" && model !== "auto" ? ["--model", model] : [];
      // agent [--model <model>] chat "<prompt>" — opções globais primeiro
      const args = [...modelArgs, subcommand, prompt];

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
          // ignore
        }
        reject(error);
      };

      const resetInactivityTimeout = () => {
        clearTimeout(inactivityTimeoutId);
        inactivityTimeoutId = setTimeout(() => {
          handleReject(
            new Error(
              `Cursor Agent CLI inativo por ${inactivityTimeout / 60000} minutos. Chunks recebidos: ${chunksReceived}.`,
            ),
          );
        }, inactivityTimeout);
      };

      try {
        child = spawn(cliPath, args, {
          cwd,
          env: process.env,
          shell: platform() === "win32",
          stdio: ["ignore", "pipe", "pipe"],
        });

        child.stdout?.on("data", (data) => {
          const chunk = data.toString();
          stdout += chunk;
          chunksReceived++;
          anyOutputReceived = true;
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
            handleResolve(stdout);
          } else {
            handleReject(
              new Error(
                stderr || `Cursor Agent CLI exited with code ${code}.`,
              ),
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
                "Nenhum dado recebido do Cursor Agent CLI dentro do tempo máximo. " +
                  "Verifique se está logado no terminal (rode 'agent' se necessário) e aumente o timeout se precisar.",
              ),
            );
          } else {
            handleReject(
              new Error(
                `Cursor Agent CLI timeout máximo (${maxTimeout / 60000} minutos). Chunks recebidos: ${chunksReceived}.`,
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

export function createCursorAdapter(provider: Provider): CursorAdapter {
  return new CursorAdapter(provider);
}
