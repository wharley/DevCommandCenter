/**
 * Base Adapter - Classe base abstrata para adapters de IA
 */

import { missionPlanSchema, generatedCodeSchema } from "../schemas";
import type {
  AIProviderAdapter,
  ValidationResult,
  AdapterConfig,
  AIResponse,
  MissionPlan,
  GeneratedCode,
  Provider,
  ProgressCallback,
} from "../types";

export abstract class BaseAdapter implements AIProviderAdapter {
  abstract readonly name: string;
  abstract readonly type: Provider["type"];

  protected provider: Provider;

  constructor(provider: Provider) {
    this.provider = provider;
  }

  abstract validate(): ValidationResult;

  abstract generatePlan(
    config: AdapterConfig,
    onProgress?: ProgressCallback
  ): Promise<AIResponse<MissionPlan>>;

  abstract generateCode(
    config: AdapterConfig,
    onProgress?: ProgressCallback
  ): Promise<AIResponse<GeneratedCode>>;

  abstract testConnection(): Promise<{ success: boolean; message: string }>;

  /**
   * Monta o prompt base para geração de plano
   */
  protected buildPlanPrompt(config: AdapterConfig): string {
    const { mission, projectContext, planFeedback } = config;

    const feedbackSection = planFeedback?.trim()
      ? `

## Feedback sobre o plano anterior
O usuário não estava satisfeito com um plano anterior. Sua solicitação:
${planFeedback.trim()}

Gere um NOVO plano que considere este feedback, mantendo o objetivo original da missão.
`
      : "";

    return `You are an expert software engineer. Analyze the following task and create a detailed implementation plan.

## Project Context
- **Project Name**: ${projectContext.projectName}
- **Project Path**: ${projectContext.projectPath}
${
  projectContext.gitInfo
    ? `- **Git Branch**: ${projectContext.gitInfo.branch}`
    : ""
}

## Available Files
${projectContext.files.slice(0, 50).join("\n")}
${
  projectContext.files.length > 50
    ? `\n... and ${projectContext.files.length - 50} more files`
    : ""
}

## Task
**Title**: ${mission.title}

**Description**: 
${mission.description}
${
  mission.preserveInstructions?.trim()
    ? `

## Preserve / Do not change
${mission.preserveInstructions.trim()}
Do not modify or suggest changes to the above.`
    : ""
}
${feedbackSection}
## Instructions
Create a detailed implementation plan with the following JSON structure:
{
  "summary": "Brief summary of the implementation approach",
  "estimatedComplexity": "low" | "medium" | "high",
  "steps": [
    {
      "id": "step-1",
      "order": 1,
      "title": "Step title",
      "description": "Detailed description of what needs to be done",
      "files": ["list", "of", "affected", "files"],
      "status": "pending"
    }
  ]
}

Respond ONLY with valid JSON. Do not include any other text or markdown code blocks.`;
  }

  /**
   * Monta o prompt para geração de código
   */
  protected buildCodePrompt(config: AdapterConfig): string {
    const { mission, projectContext } = config;
    const plan = mission.plan;

    return `You are an expert software engineer. Based on the implementation plan, generate the code changes needed.

## Project Context
- **Project Name**: ${projectContext.projectName}
- **Project Path**: ${projectContext.projectPath}

## Implementation Plan
${plan ? JSON.stringify(plan, null, 2) : "No plan available"}

## Task
**Title**: ${mission.title}
**Description**: ${mission.description}
${
  mission.preserveInstructions?.trim()
    ? `

## Preserve / Do not change
${mission.preserveInstructions.trim()}
Do not modify or suggest changes to the above.`
    : ""
}

## Instructions
Generate the code changes with the following JSON structure:
{
  "summary": "Summary of changes made",
  "files": [
    {
      "path": "relative/path/to/file.ts",
      "action": "create" | "modify" | "delete",
      "originalContent": "optional - helps generate accurate diff when modifying",
      "suggestedContent": "required for create and modify (guarantees fallback when diff fails)",
      "diff": "required for modify - unified diff (lines with +, -, ---, +++); optional for create"
    }
  ]
}

Rules by action:
- modify: diff is REQUIRED. suggestedContent is REQUIRED (fallback when diff fails).
- create: suggestedContent is REQUIRED (full file content). diff is optional.
- delete: path is REQUIRED. The file will be removed.

Important:
- Use proper indentation and formatting
- Follow the project's existing code style
- Include all necessary imports
- The "diff" field must contain ONLY a unified diff. Do NOT put explanatory text or comments in diff.
- Respond ONLY with valid JSON. Do not include any other text or markdown code blocks.
- Output only a single JSON object. Do not wrap it in markdown code blocks or add any text before or after.`;
  }

  /**
   * Extrai o objeto JSON de nível superior por contagem de chaves (ignora { } dentro de strings).
   */
  private extractTopLevelJson(str: string): string | null {
    const start = str.indexOf("{");
    if (start === -1) return null;
    let depth = 0;
    let inString = false;
    let escape = false;
    const quote = '"';
    for (let i = start; i < str.length; i++) {
      const c = str[i];
      if (inString) {
        if (escape) {
          escape = false;
          continue;
        }
        if (c === "\\") {
          escape = true;
          continue;
        }
        if (c === quote) {
          inString = false;
          continue;
        }
        continue;
      }
      if (c === quote) {
        inString = true;
        continue;
      }
      if (c === "{") {
        depth++;
        continue;
      }
      if (c === "}") {
        depth--;
        if (depth === 0) return str.slice(start, i + 1);
        continue;
      }
    }
    return null;
  }

  /**
   * Dentro de strings JSON, escapa aspas duplas que o LLM esqueceu de escapar (ex.: código com "use strict").
   * Só escapa quando a aspa não é o delimitador de fechamento (próximo char é estrutural: , } ] : ou espaço).
   */
  private fixUnescapedQuotesInJsonStrings(str: string): string {
    let inString = false;
    let escape = false;
    const result: string[] = [];
    const structuralAfterString = /^[\s,}\]:]/;
    for (let i = 0; i < str.length; i++) {
      const c = str[i];
      const next = str[i + 1];
      if (inString) {
        if (escape) {
          result.push(c);
          escape = false;
          continue;
        }
        if (c === "\\") {
          result.push(c);
          escape = true;
          continue;
        }
        if (c === '"') {
          if (structuralAfterString.test(next ?? "")) {
            result.push(c);
            inString = false;
          } else {
            result.push("\\", c);
          }
          continue;
        }
        result.push(c);
        continue;
      }
      if (c === '"') {
        result.push(c);
        inString = true;
        continue;
      }
      result.push(c);
    }
    return result.join("");
  }

  /**
   * Substitui newlines literais dentro de strings JSON por \\n (comum em saídas de LLM).
   * Percorre o texto e, apenas dentro de strings entre aspas duplas, troca \\n e \\r por \\n.
   */
  private fixUnescapedNewlinesInJsonStrings(str: string): string {
    let inString = false;
    let escape = false;
    const result: string[] = [];
    for (let i = 0; i < str.length; i++) {
      const c = str[i];
      const code = c.charCodeAt(0);
      if (inString) {
        if (escape) {
          result.push(c);
          escape = false;
          continue;
        }
        if (c === "\\") {
          result.push(c);
          escape = true;
          continue;
        }
        if (c === '"') {
          result.push(c);
          inString = false;
          continue;
        }
        if (code === 10 || code === 13) {
          result.push("\\n");
          if (code === 13 && str[i + 1] === "\n") i++;
          continue;
        }
        result.push(c);
        continue;
      }
      if (c === '"') {
        result.push(c);
        inString = true;
        continue;
      }
      result.push(c);
    }
    return result.join("");
  }

  /**
   * Tenta parsear JSON aplicando correções de newlines e aspas não escapadas (comum em LLM).
   * Usado pelo Cursor adapter para parsear o inner payload dentro do wrapper do CLI.
   */
  protected tryParseWithFixes(json: string): unknown | null {
    const trimmed = json.trim();
    if (!trimmed) return null;

    try {
      return JSON.parse(trimmed) as unknown;
    } catch {
      // segue
    }

    try {
      const fixed = this.fixUnescapedNewlinesInJsonStrings(trimmed);
      return JSON.parse(fixed) as unknown;
    } catch {
      // segue
    }

    try {
      const fixedNewlines = this.fixUnescapedNewlinesInJsonStrings(trimmed);
      const fixedQuotes = this.fixUnescapedQuotesInJsonStrings(fixedNewlines);
      return JSON.parse(fixedQuotes) as unknown;
    } catch {
      // segue
    }

    const byBrace = this.extractTopLevelJson(trimmed);
    if (byBrace) {
      try {
        return JSON.parse(byBrace) as unknown;
      } catch {
        // segue
      }
      try {
        return JSON.parse(
          this.fixUnescapedNewlinesInJsonStrings(byBrace)
        ) as unknown;
      } catch {
        // segue
      }
      try {
        const fixedBoth = this.fixUnescapedQuotesInJsonStrings(
          this.fixUnescapedNewlinesInJsonStrings(byBrace)
        );
        return JSON.parse(fixedBoth) as unknown;
      } catch {
        // segue
      }
    }

    return null;
  }

  /**
   * Extrai o JSON de nível superior de uma string (para uso em fallbacks).
   */
  protected extractTopLevelJsonProtected(str: string): string | null {
    return this.extractTopLevelJson(str);
  }

  /**
   * Tenta fazer parse de JSON de uma resposta que pode ter texto adicional
   */
  protected parseJSONResponse<T>(response: string): T | null {
    const trimmed = response.trim();

    // 1. Parse direto
    try {
      return JSON.parse(trimmed) as T;
    } catch {
      // segue
    }

    // 2. Newlines literais dentro de strings (quebra JSON; comum em suggestedContent)
    try {
      const fixed = this.fixUnescapedNewlinesInJsonStrings(trimmed);
      return JSON.parse(fixed) as T;
    } catch {
      // segue
    }

    // 2b. Newlines + aspas não escapadas (código com "use strict" etc.)
    try {
      const fixedNewlines = this.fixUnescapedNewlinesInJsonStrings(trimmed);
      const fixedQuotes = this.fixUnescapedQuotesInJsonStrings(fixedNewlines);
      return JSON.parse(fixedQuotes) as T;
    } catch {
      // segue
    }

    // 3. Bloco markdown: conteúdo após ```json ou ``` (pode conter backticks no conteúdo)
    const codeBlockOpen = /^```(?:json)?\s*\n?/;
    if (codeBlockOpen.test(trimmed)) {
      const afterFence = trimmed.replace(codeBlockOpen, "").trim();
      const content = afterFence.endsWith("```")
        ? afterFence.slice(0, -3).trim()
        : afterFence;
      try {
        const extracted = this.extractTopLevelJson(content) ?? content;
        return JSON.parse(extracted) as T;
      } catch {
        // segue
      }
      try {
        const fixed = this.fixUnescapedNewlinesInJsonStrings(content);
        return JSON.parse(this.extractTopLevelJson(fixed) ?? fixed) as T;
      } catch {
        // segue
      }
    }

    // 4. Extração por contagem de chaves no texto inteiro
    const byBrace = this.extractTopLevelJson(trimmed);
    if (byBrace) {
      try {
        return JSON.parse(byBrace) as T;
      } catch {
        // segue
      }
      try {
        return JSON.parse(this.fixUnescapedNewlinesInJsonStrings(byBrace)) as T;
      } catch {
        // segue
      }
      try {
        const fixedBoth = this.fixUnescapedQuotesInJsonStrings(
          this.fixUnescapedNewlinesInJsonStrings(byBrace)
        );
        return JSON.parse(fixedBoth) as T;
      } catch {
        // segue
      }
    }

    // 4b. Fix newlines no texto inteiro e depois extrair JSON (cobre suggestedContent com newlines literais)
    try {
      const fixedTrimmed = this.fixUnescapedNewlinesInJsonStrings(trimmed);
      const byBraceFromFixed = this.extractTopLevelJson(fixedTrimmed);
      if (byBraceFromFixed) {
        try {
          return JSON.parse(byBraceFromFixed) as T;
        } catch {
          // segue
        }
        try {
          const fixedQuotes =
            this.fixUnescapedQuotesInJsonStrings(byBraceFromFixed);
          return JSON.parse(fixedQuotes) as T;
        } catch {
          // segue
        }
      }
    } catch {
      // segue
    }

    // 5. Regex guloso como último recurso
    const jsonMatch = trimmed.match(/\{[\s\S]*\}/);
    if (jsonMatch) {
      const raw = jsonMatch[0];
      try {
        return JSON.parse(raw) as T;
      } catch {
        // segue
      }
      try {
        return JSON.parse(this.fixUnescapedNewlinesInJsonStrings(raw)) as T;
      } catch {
        // segue
      }
      try {
        return JSON.parse(
          this.fixUnescapedQuotesInJsonStrings(
            this.fixUnescapedNewlinesInJsonStrings(raw)
          )
        ) as T;
      } catch {
        // segue
      }
    }

    // 6. Reparo por truncamento: resposta cortada no meio de suggestedContent (fecha string + objeto atual + array files + objeto topo)
    const trimmedEnd = trimmed.slice(-20);
    if (!trimmedEnd.trimEnd().endsWith("}")) {
      for (const suffix of ['"}]}', '"}]}]}', '"}]}]}]}', '"}]}]}]}]}']) {
        try {
          return JSON.parse(trimmed + suffix) as T;
        } catch {
          // segue
        }
        try {
          const fixed = this.fixUnescapedNewlinesInJsonStrings(trimmed);
          return JSON.parse(fixed + suffix) as T;
        } catch {
          // segue
        }
      }
    }

    return null;
  }

  /**
   * Gera IDs únicos para steps
   */
  protected generateStepId(index: number): string {
    return `step-${Date.now()}-${index}`;
  }

  /**
   * Pipeline de normalização: extrai payload, parseia com fixes, valida com Zod.
   * Aceita wrappers comuns: { result }, { output }, JSON string dentro de JSON.
   * @param raw Resposta bruta (stdout do CLI ou resposta da API)
   * @param schema Zod schema (missionPlanSchema ou generatedCodeSchema)
   * @returns Objeto validado ou erro de schema formatado
   */
  protected normalizeAndValidate<T>(
    raw: string,
    schema: {
      safeParse: (v: unknown) => {
        success: boolean;
        data?: T;
        error?: { message?: string; issues?: unknown[] };
      };
    }
  ): { success: true; data: T } | { success: false; error: string } {
    const trimmed = raw.trim();
    if (!trimmed) {
      return { success: false, error: "Empty response" };
    }

    let candidate: unknown = null;

    // 1. Unwrap common wrappers { type, result } | { type, output }
    try {
      const parsed = JSON.parse(trimmed) as {
        type?: string;
        result?: unknown;
        output?: unknown;
      };
      const inner = parsed?.result ?? parsed?.output;
      if (inner != null) {
        if (typeof inner === "string") {
          candidate = this.tryParseWithFixes(inner);
        } else if (typeof inner === "object") {
          candidate = inner;
        }
      }
    } catch {
      // não é wrapper JSON
    }

    // 2. Parse direto ou com fixes
    if (candidate == null) {
      candidate =
        this.parseJSONResponse<unknown>(trimmed) ??
        this.tryParseWithFixes(trimmed);
    }

    if (candidate == null) {
      return { success: false, error: "Could not parse JSON from response" };
    }

    // 3. Validação Zod
    const result = schema.safeParse(candidate);
    if (result.success) {
      return { success: true, data: result.data as T };
    }

    const err = result.error;
    const msg = err?.message ?? "Schema validation failed";
    const issues = err?.issues ?? [];
    const details =
      issues.length > 0
        ? ` (${issues
            .slice(0, 3)
            .map((i) =>
              typeof i === "object" && i && "message" in i
                ? (i as { message: string }).message
                : String(i)
            )
            .join("; ")})`
        : "";
    return { success: false, error: `${msg}${details}` };
  }

  /**
   * Normaliza e valida resposta como MissionPlan (com Zod).
   * Garante que cada step tenha id (usa generateStepId quando ausente).
   */
  protected parseAndValidateMissionPlan(
    raw: string
  ): { success: true; data: MissionPlan } | { success: false; error: string } {
    const result = this.normalizeAndValidate(raw, missionPlanSchema);
    if (!result.success) return result;
    const plan = result.data as MissionPlan;
    if (plan.steps) {
      plan.steps = plan.steps.map((step, index) => ({
        ...step,
        id: step.id ?? this.generateStepId(index),
      }));
    }
    return { success: true, data: plan };
  }

  /**
   * Normaliza e valida resposta como GeneratedCode (com Zod).
   */
  protected parseAndValidateGeneratedCode(
    raw: string
  ):
    | { success: true; data: GeneratedCode }
    | { success: false; error: string } {
    const result = this.normalizeAndValidate(raw, generatedCodeSchema);
    if (!result.success) return result;
    return { success: true, data: result.data as GeneratedCode };
  }
}
