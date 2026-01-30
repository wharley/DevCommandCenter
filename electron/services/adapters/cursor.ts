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
/** Número de linhas do NDJSON a incluir em erro para diagnóstico (truncamento vs formato). */
const NDJSON_DIAGNOSTIC_LINES = 30;

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
      let response = await this.executeAgentCommand(
        "chat",
        prompt,
        config.projectContext.projectPath,
        onProgress,
      );
      response = this.unwrapCursorCliResponse(response);

      onProgress?.("Processando resposta...");
      let plan = this.parseJSONResponse<MissionPlan>(response);

      // Fallback: response may be full CLI wrapper (type, result) with result as string or object
      if (!plan || !Array.isArray(plan.steps)) {
        try {
          const parsed = JSON.parse(response) as { type?: string; result?: unknown; output?: unknown };
          const inner = parsed?.result ?? parsed?.output;
          if (typeof inner === "string") {
            plan = this.parseJSONResponse<MissionPlan>(inner) ?? plan ?? null;
          } else if (inner && typeof inner === "object" && Array.isArray((inner as MissionPlan).steps)) {
            plan = inner as MissionPlan;
          }
        } catch {
          // ignore
        }
        // Fallback NDJSON: resposta multi-linha (extractPayload devolveu stdout inteiro)
        if ((!plan || !Array.isArray(plan.steps)) && response.includes("\n")) {
          const ndjsonPayload = this.tryExtractPayloadFromNDJSON(response);
          if (ndjsonPayload) plan = this.parseJSONResponse<MissionPlan>(ndjsonPayload) ?? plan ?? null;
        }
      }

      if (!plan || !Array.isArray(plan.steps)) {
        const lines = response.trim().split(/\r?\n/).filter((l) => l.length > 0);
        const looksNdjson = lines.length > 1;
        const lastLineLooksResult = lines.length > 0 && lines[lines.length - 1]!.includes('"type"') && lines[lines.length - 1]!.includes('"result"');
        const truncationHint = looksNdjson && lastLineLooksResult
          ? " Cursor CLI result line may be truncated (incomplete JSON)."
          : "";
        const diagnosticHint = looksNdjson
          ? ` For diagnosis, save the last ${NDJSON_DIAGNOSTIC_LINES} lines of the CLI output to a file.`
          : "";
        return {
          success: false,
          error: `Failed to parse plan from Cursor response.${truncationHint}${diagnosticHint}`,
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
      let response = await this.executeAgentCommand(
        "chat",
        prompt,
        config.projectContext.projectPath,
        onProgress,
      );
      response = this.unwrapCursorCliResponse(response);

      onProgress?.("Processando resposta...");
      let code = this.parseJSONResponse<GeneratedCode>(response);

      if (!code) {
        // Fallback 1: response may be wrapper JSON (e.g. { result: "<string>" } or { result: <object> })
        try {
          const parsed = JSON.parse(response) as {
            type?: string;
            result?: unknown;
            output?: unknown;
          };
          const inner = parsed?.result ?? parsed?.output;
          if (
            inner &&
            typeof inner === "object" &&
            Array.isArray((inner as GeneratedCode).files)
          ) {
            code = inner as GeneratedCode;
          } else if (typeof inner === "string") {
            // Cursor CLI wrapper: result is the JSON string of GeneratedCode
            code = this.parseJSONResponse<GeneratedCode>(inner);
          }
        } catch {
          // ignore
        }
        // Fallback NDJSON: resposta multi-linha (extractPayload devolveu stdout inteiro)
        if (!code && response.includes("\n")) {
          const ndjsonPayload = this.tryExtractPayloadFromNDJSON(response);
          if (ndjsonPayload) code = this.parseJSONResponse<GeneratedCode>(ndjsonPayload);
        }
      }

      // Normalize wrapper: CLI may return { result: GeneratedCode } or { output: GeneratedCode }
      if (code && "result" in code && Array.isArray((code as { result?: GeneratedCode }).result?.files)) {
        code = (code as { result: GeneratedCode }).result;
      } else if (code && "output" in code && Array.isArray((code as { output?: GeneratedCode }).output?.files)) {
        code = (code as { output: GeneratedCode }).output;
      }

      // Fallback 2: response was the full CLI wrapper (type, subtype, result) - parseJSONResponse returned it as "code"
      if (
        code &&
        typeof code === "object" &&
        "type" in code &&
        (code as { type?: string }).type === "result" &&
        ("result" in code || "output" in code)
      ) {
        const raw = (code as { result?: unknown; output?: unknown }).result ?? (code as { result?: unknown; output?: unknown }).output;
        if (typeof raw === "string") {
          code = this.parseJSONResponse<GeneratedCode>(raw) ?? code;
          if (!code || !Array.isArray(code.files)) {
            const parsedWithFixes = this.tryParseWithFixes(raw);
            if (parsedWithFixes && typeof parsedWithFixes === "object" && Array.isArray((parsedWithFixes as GeneratedCode).files)) {
              code = parsedWithFixes as GeneratedCode;
            }
          }
          if (!code || !Array.isArray(code.files)) {
            const extracted = this.extractTopLevelJsonProtected(raw);
            if (extracted) {
              const parsed = this.parseJSONResponse<GeneratedCode>(extracted) ?? this.tryParseWithFixes(extracted) as GeneratedCode | null;
              if (parsed && Array.isArray(parsed?.files)) code = parsed;
            }
          }
          if ((!code || !Array.isArray(code.files)) && raw.includes("\n")) {
            const ndjsonPayload = this.tryExtractPayloadFromNDJSON(raw);
            if (ndjsonPayload) {
              const parsed = this.parseJSONResponse<GeneratedCode>(ndjsonPayload);
              if (parsed?.files) code = parsed;
            }
          }
        } else if (raw && typeof raw === "object" && Array.isArray((raw as GeneratedCode).files)) {
          code = raw as GeneratedCode;
        }
      }

      if (!code || !Array.isArray(code.files)) {
        const lines = response.trim().split(/\r?\n/).filter((l) => l.length > 0);
        const looksNdjson = lines.length > 1;
        const lastLineLooksResult = lines.length > 0 && lines[lines.length - 1]!.includes('"type"') && lines[lines.length - 1]!.includes('"result"');
        const truncationHint = looksNdjson && lastLineLooksResult
          ? " Cursor CLI result line may be truncated (incomplete JSON)."
          : "";
        const diagnosticHint = looksNdjson
          ? ` For diagnosis, save the last ${NDJSON_DIAGNOSTIC_LINES} lines of the CLI output to a file.`
          : "";
        const snippet = response.slice(0, 600).trim();
        return {
          success: false,
          error: `Failed to parse code from Cursor response.${truncationHint}${diagnosticHint} Raw response snippet: ${snippet}${response.length > 600 ? "..." : ""}`,
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
   * Desembrulha a resposta do Cursor CLI quando vem no formato wrapper (type/result).
   * Se o response for o wrapper completo, extrai o payload interno (result/output) e retorna
   * como string JSON normalizada. Caso contrário retorna o response inalterado.
   */
  private unwrapCursorCliResponse(response: string): string {
    const trimmed = response.trim();
    if (!trimmed) return response;

    try {
      const wrapper = JSON.parse(trimmed) as {
        type?: string;
        result?: string | { summary?: string; files?: unknown[]; steps?: unknown[] };
        output?: string | { summary?: string; files?: unknown[]; steps?: unknown[] };
      };
      if (wrapper?.type !== "result") return response;

      const raw = wrapper?.result ?? wrapper?.output;
      if (typeof raw === "string") {
        const inner = this.tryParseWithFixes(raw);
        if (inner != null && typeof inner === "object") {
          return JSON.stringify(inner);
        }
        return raw;
      }
      if (
        raw &&
        typeof raw === "object" &&
        (Array.isArray((raw as { files?: unknown[] }).files) ||
          Array.isArray((raw as { steps?: unknown[] }).steps))
      ) {
        return JSON.stringify(raw);
      }
    } catch {
      // não é JSON único; tentar NDJSON (última linha com type=result)
    }

    if (trimmed.includes("\n")) {
      const ndjsonPayload = this.tryExtractPayloadFromNDJSON(trimmed);
      if (ndjsonPayload) return ndjsonPayload;
    }

    return response;
  }

  /**
   * Extrai o payload útil da stdout do Cursor CLI.
   *
   * Documentação: https://docs.cursor.com/cli/reference/output-format
   *
   * O CLI pode emitir NDJSON (um JSON por linha). A linha final costuma ser:
   *   {"type":"result","subtype":"success", ... "result":"{...}"}
   * O campo `result` não é um objeto: é uma string que contém JSON escapado (\"summary\", \\n, etc).
   *
   * Fluxo correto (duplo parse):
   * 1. Ler stdout linha por linha (NDJSON) ou como único JSON.
   * 2. JSON.parse(linha) ou JSON.parse(stdout).
   * 3. Quando type === "result", pegar obj.result (ou obj.output).
   * 4. Se result for string: segundo parse — JSON.parse(obj.result) — para obter o objeto interno.
   *
   * Com --output-format json o CLI pode retornar um único objeto ou NDJSON; esta função
   * aplica o duplo parse quando result é string e devolve o JSON interno normalizado (string)
   * para o caller fazer parseJSONResponse uma vez.
   *
   * Em caso de falha de parse no app, para diagnóstico: salvar as últimas NDJSON_DIAGNOSTIC_LINES
   * linhas do stdout (ex.: tail -n 30 out.ndjson) e o comando exato (incl. --output-format json).
   */
  private extractPayloadFromCursorStdout(stdout: string): string {
    const trimmed = stdout.trim();
    if (!trimmed) return trimmed;

    // 1. Tentar parse como JSON único (formato oficial com type/result)
    try {
      const wrapper = JSON.parse(trimmed) as {
        type?: string;
        result?: string | { summary?: string; files?: unknown[]; steps?: unknown[] };
        output?: string | { summary?: string; files?: unknown[]; steps?: unknown[] };
      };
      const raw = wrapper?.result ?? wrapper?.output;
      if (typeof raw === "string") {
        // Duplo parse: result é string com JSON escapado → segundo parse aqui e devolver normalizado
        try {
          const inner = JSON.parse(raw) as unknown;
          return JSON.stringify(inner);
        } catch {
          const innerWithFixes = this.tryParseWithFixes(raw);
          if (innerWithFixes != null && typeof innerWithFixes === "object") {
            return JSON.stringify(innerWithFixes);
          }
          return raw;
        }
      }
      if (
        raw &&
        typeof raw === "object" &&
        Array.isArray((raw as { files?: unknown[] }).files)
      ) {
        return JSON.stringify(raw);
      }
      if (
        raw &&
        typeof raw === "object" &&
        Array.isArray((raw as { steps?: unknown[] }).steps)
      ) {
        return JSON.stringify(raw);
      }
    } catch {
      // não é um único JSON; tentar NDJSON
    }

    // 2. NDJSON: última linha que seja JSON com type:"result"
    const lines = trimmed.split(/\r?\n/).filter((l) => l.trim().length > 0);
    for (let i = lines.length - 1; i >= 0; i--) {
      try {
        const wrapper = JSON.parse(lines[i]!) as {
          type?: string;
          result?: string | { summary?: string; files?: unknown[]; steps?: unknown[] };
          output?: string | { summary?: string; files?: unknown[]; steps?: unknown[] };
        };
        if (wrapper?.type !== "result") continue;
        const raw = wrapper?.result ?? wrapper?.output;
        if (typeof raw === "string") {
          // Duplo parse: result é string com JSON escapado → segundo parse aqui e devolver normalizado
          try {
            const inner = JSON.parse(raw) as unknown;
            return JSON.stringify(inner);
          } catch {
            const innerWithFixes = this.tryParseWithFixes(raw);
            if (innerWithFixes != null && typeof innerWithFixes === "object") {
              return JSON.stringify(innerWithFixes);
            }
            return raw;
          }
        }
        if (
          raw &&
          typeof raw === "object" &&
          Array.isArray((raw as { files?: unknown[] }).files)
        ) {
          return JSON.stringify(raw);
        }
        if (
          raw &&
          typeof raw === "object" &&
          Array.isArray((raw as { steps?: unknown[] }).steps)
        ) {
          return JSON.stringify(raw);
        }
      } catch {
        continue;
      }
    }

    return trimmed;
  }

  /**
   * Tenta reparar uma linha NDJSON truncada (ex.: última linha cortada) fechando chaves/aspas.
   * Se conseguir parsear e tiver type==="result", devolve o payload interno normalizado.
   */
  private tryRepairTruncatedResultLine(line: string): string | null {
    const suffixes = ['"}}', '"}]}}', "}}", "}]}", "}]}]}"];
    for (const suffix of suffixes) {
      try {
        const repaired = line.trimEnd() + suffix;
        const wrapper = JSON.parse(repaired) as {
          type?: string;
          result?: string | { summary?: string; files?: unknown[]; steps?: unknown[] };
          output?: string | { summary?: string; files?: unknown[]; steps?: unknown[] };
        };
        if (wrapper?.type !== "result") continue;
        const raw = wrapper?.result ?? wrapper?.output;
        if (typeof raw === "string") {
          try {
            const inner = JSON.parse(raw) as unknown;
            return JSON.stringify(inner);
          } catch {
            const innerWithFixes = this.tryParseWithFixes(raw);
            if (innerWithFixes != null && typeof innerWithFixes === "object") {
              return JSON.stringify(innerWithFixes);
            }
            return raw;
          }
        }
        if (
          raw &&
          typeof raw === "object" &&
          (Array.isArray((raw as { files?: unknown[] }).files) ||
            Array.isArray((raw as { steps?: unknown[] }).steps))
        ) {
          return JSON.stringify(raw);
        }
      } catch {
        continue;
      }
    }
    return null;
  }

  /**
   * Tenta extrair o payload interno de uma resposta multi-linha (NDJSON) quando
   * extractPayloadFromCursorStdout devolveu o stdout inteiro (ex.: última linha truncada).
   * Itera as linhas, faz primeiro parse por linha; na linha com type==="result" pega result/output;
   * se for string, faz segundo parse (duplo parse) e devolve o JSON interno normalizado.
   */
  private tryExtractPayloadFromNDJSON(response: string): string | null {
    const trimmed = response.trim();
    const lines = trimmed.split(/\r?\n/).filter((l) => l.trim().length > 0);
    if (lines.length < 2) return null;
    for (let i = lines.length - 1; i >= 0; i--) {
      try {
        const wrapper = JSON.parse(lines[i]!) as {
          type?: string;
          result?: string | { summary?: string; files?: unknown[]; steps?: unknown[] };
          output?: string | { summary?: string; files?: unknown[]; steps?: unknown[] };
        };
        if (wrapper?.type !== "result") continue;
        const raw = wrapper?.result ?? wrapper?.output;
        if (typeof raw === "string") {
          try {
            const inner = JSON.parse(raw) as unknown;
            return JSON.stringify(inner);
          } catch {
            const innerWithFixes = this.tryParseWithFixes(raw);
            if (innerWithFixes != null && typeof innerWithFixes === "object") {
              return JSON.stringify(innerWithFixes);
            }
            return raw;
          }
        }
        if (
          raw &&
          typeof raw === "object" &&
          (Array.isArray((raw as { files?: unknown[] }).files) ||
            Array.isArray((raw as { steps?: unknown[] }).steps))
        ) {
          return JSON.stringify(raw);
        }
      } catch {
        // Última linha pode estar truncada; tentar reparar
        if (i === lines.length - 1 && lines[i]!.includes('"type"') && lines[i]!.includes('"result"')) {
          const repaired = this.tryRepairTruncatedResultLine(lines[i]!);
          if (repaired) return repaired;
        }
        continue;
      }
    }
    return null;
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
      // agent [--model <model>] [--output-format json] chat "<prompt>" — JSON para parse do plano/código.
      // Se a doc do Cursor indicar que NDJSON vem com --output-format stream-json, usar stream-json aqui
      // e manter a mesma lógica (NDJSON + duplo parse na linha type=result).
      const args = [
        ...modelArgs,
        "--output-format",
        "json",
        subcommand,
        prompt,
      ];

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
            const payload = this.extractPayloadFromCursorStdout(stdout);
            handleResolve(payload);
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
