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
      let planResult = this.parseAndValidateMissionPlan(response);

      // Fallback NDJSON: quando unwrap devolveu stdout inteiro
      if (!planResult.success && response.includes("\n")) {
        const ndjsonPayload = this.tryExtractPayloadFromNDJSON(response);
        if (ndjsonPayload) {
          planResult = this.parseAndValidateMissionPlan(ndjsonPayload);
        }
      }

      if (!planResult.success) {
        const lines = response.trim().split(/\r?\n/).filter((l) => l.length > 0);
        const looksNdjson = lines.length > 1;
        const lastLineLooksResult =
          lines.length > 0 &&
          lines[lines.length - 1]!.includes('"type"') &&
          lines[lines.length - 1]!.includes('"result"');
        const truncationHint =
          looksNdjson && lastLineLooksResult
            ? " Cursor CLI result line may be truncated (incomplete JSON)."
            : "";
        const diagnosticHint = looksNdjson
          ? ` For diagnosis, save the last ${NDJSON_DIAGNOSTIC_LINES} lines of the CLI output to a file.`
          : "";
        return {
          success: false,
          error: `Failed to parse plan: ${planResult.error}${truncationHint}${diagnosticHint}`,
          metadata: {
            durationMs: Date.now() - startTime,
            provider: this.name,
          },
        };
      }

      let plan = planResult.data;
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
      let codeResult = this.parseAndValidateGeneratedCode(response);

      // Fallback NDJSON: quando unwrap devolveu stdout inteiro
      if (!codeResult.success && response.includes("\n")) {
        const ndjsonPayload = this.tryExtractPayloadFromNDJSON(response);
        if (ndjsonPayload) {
          codeResult = this.parseAndValidateGeneratedCode(ndjsonPayload);
        }
      }

      if (!codeResult.success) {
        const lines = response.trim().split(/\r?\n/).filter((l) => l.length > 0);
        const looksNdjson = lines.length > 1;
        const lastLineLooksResult =
          lines.length > 0 &&
          lines[lines.length - 1]!.includes('"type"') &&
          lines[lines.length - 1]!.includes('"result"');
        const truncationHint =
          looksNdjson && lastLineLooksResult
            ? " Cursor CLI result line may be truncated (incomplete JSON)."
            : "";
        const diagnosticHint = looksNdjson
          ? ` For diagnosis, save the last ${NDJSON_DIAGNOSTIC_LINES} lines of the CLI output to a file.`
          : "";
        const snippet = response.slice(0, 600).trim();
        return {
          success: false,
          error: `Failed to parse code: ${codeResult.error}${truncationHint}${diagnosticHint} Raw snippet: ${snippet}${response.length > 600 ? "..." : ""}`,
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
      // JSON único falhou; tentar reparar (truncamento) ou NDJSON
    }

    // 1. Linha única truncada: tentar reparo (funciona para single-line também)
    if (trimmed.includes('"type"') && trimmed.includes('"result"')) {
      const repaired = this.tryRepairTruncatedResultLine(trimmed);
      if (repaired) return repaired;

      // 1b. Extração manual do "result" (wrapper truncado no meio do JSON interno)
      const manualInner = this.tryExtractResultValueManually(trimmed);
      if (manualInner) {
        const parsed = this.tryParseWithFixes(manualInner);
        if (parsed != null && typeof parsed === "object") {
          return JSON.stringify(parsed);
        }
        const extracted = this.extractTopLevelJsonProtected(manualInner);
        if (extracted) {
          const parsedExtracted = this.tryParseWithFixes(extracted);
          if (parsedExtracted != null && typeof parsedExtracted === "object") {
            return JSON.stringify(parsedExtracted);
          }
        }
        return manualInner;
      }
    }

    // 2. NDJSON: múltiplas linhas
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
   * Extrai manualmente o valor de "result" ou "output" de uma string que parece wrapper
   * mas não parseia (ex.: truncada). Útil quando tryRepairTruncatedResultLine falha.
   */
  private tryExtractResultValueManually(str: string): string | null {
    const match = str.match(/"result"\s*:\s*"|"output"\s*:\s*"/);
    if (!match) return null;
    const start = (match.index ?? 0) + match[0].length;
    let i = start;
    while (i < str.length) {
      const c = str[i];
      if (c === "\\") {
        i += 2;
        continue;
      }
      if (c === '"') {
        const inner = str.slice(start, i);
        if (inner.startsWith("{") && (inner.includes("summary") || inner.includes("files"))) {
          return inner;
        }
        return null;
      }
      i++;
    }
    // Truncado: não encontrou aspas de fechamento; usar o resto da string
    const inner = str.slice(start);
    if (inner.startsWith("{") && inner.length > 20) return inner;
    return null;
  }

  /**
   * Tenta reparar uma linha NDJSON truncada (ex.: última linha cortada) fechando chaves/aspas.
   * Se conseguir parsear e tiver type==="result", devolve o payload interno normalizado.
   */
  private tryRepairTruncatedResultLine(line: string): string | null {
    const suffixes = [
      '"}}',
      '"}]}}',
      '"}]}',
      '"}]}]}',
      '"}]}]}]}',
      "}}",
      "}]}",
      "}]}]}",
      "}]}]}]}",
    ];
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
