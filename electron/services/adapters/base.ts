/**
 * Base Adapter - Classe base abstrata para adapters de IA
 */

import { missionPlanSchema, generatedCodeSchema } from "../schemas";
import {
  extractCommandsFromPlan,
  extractCommandsFromCode,
  mergeCommands,
} from "../../../lib/command-detector";
import type {
  AIProviderAdapter,
  ValidationResult,
  AdapterConfig,
  AIResponse,
  MissionPlan,
  GeneratedCode,
  Provider,
  ProgressCallback,
  PendingCommand,
} from "../types";

// ---------------------------------------------------------------------------
// Plan parse guard-rail: warnings and user-facing message
// ---------------------------------------------------------------------------

export type PlanRepairWarning =
  | { type: "EMPTY_STEPS_DROPPED" }
  | { type: "STEP_COERCED_TO_NUMBER"; stepIndex: number; rawOrder: unknown }
  | { type: "STEP_MISSING_ORDER"; stepIndex: number }
  | { type: "STEP_MISSING_REQUIRED_FIELDS"; stepIndex: number; missing: string[] }
  | { type: "NON_ARRAY_STEPS_COERCED" }
  | { type: "UNKNOWN_SHAPE"; detail?: string };

export const PLAN_PARSE_ERROR_USER_MESSAGE =
  "Não foi possível processar o plano. Tente regenerar o plano.";

export const PLAN_RETRY_HINT =
  "The previous response was not valid JSON. Reply with only the MissionPlan JSON object (keys: summary, estimatedComplexity, steps). No markdown, no extra text.";

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
    const { mission, projectContext, planFeedback, planRetryHint } = config;

    const feedbackSection = planFeedback?.trim()
      ? `

## Feedback on Previous Plan
The user was not satisfied with a previous plan. Their request:
${planFeedback.trim()}

Generate a NEW plan that considers this feedback while maintaining the original mission objective.
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
  "summary": "Brief summary of the implementation approach (write in Portuguese for user display)",
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

Respond ONLY with valid JSON. Do not include any other text or markdown code blocks.${
      planRetryHint?.trim()
        ? `

## Important
${planRetryHint.trim()}`
        : ""
    }`;
  }

  /**
   * Monta o prompt para geração de código
   * Optimized: requests diff-only for modify operations to reduce token output
   */
  protected buildCodePrompt(config: AdapterConfig): string {
    const { mission, projectContext, codeFeedback } = config;
    const plan = mission.plan;

    const feedbackSection = codeFeedback?.trim()
      ? `

## Feedback on Previous Attempt
The previous code generation did not meet expectations. User feedback:
${codeFeedback.trim()}

Please generate the code addressing this issue. Pay special attention to the mentioned details.
`
      : "";

    // Pilar 2: Conteúdo dos arquivos a modificar (para diffs mais precisos)
    const fileContentsSection =
      projectContext.fileContents &&
      Object.keys(projectContext.fileContents).length > 0
        ? `

## Files to Modify (current content)
Use this content as the base for generating accurate diffs and suggestedContent.

${Object.entries(projectContext.fileContents)
  .map(
    ([path, content]) =>
      `### ${path}\n\`\`\`\n${content}\n\`\`\``
  )
  .join("\n\n")}
`
        : "";

    return `You are an expert software engineer. Based on the implementation plan, generate the code changes needed.

## Project Context
- **Project Name**: ${projectContext.projectName}
- **Project Path**: ${projectContext.projectPath}

## Implementation Plan
${plan ? JSON.stringify(plan, null, 2) : "No plan available"}${fileContentsSection}

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
}${feedbackSection}

## Instructions
Generate the code changes with the following JSON structure:
{
  "summary": "Summary of changes made (write in Portuguese for user display)",
  "files": [
    {
      "path": "relative/path/to/file.ts",
      "action": "create" | "modify" | "delete",
      "originalContent": "optional - the original content before changes (helps generate accurate diff)",
      "suggestedContent": "REQUIRED for create and modify - always include full file content (ensures reliable apply)",
      "diff": "REQUIRED for modify - unified diff format (---, +++, @@, +, -). Optional for create."
    }
  ]
}

Rules by action (Pilar 3 - suggestedContent required for reliable apply):
- modify: "suggestedContent" is REQUIRED (full file content). "diff" is REQUIRED (unified diff format).
- create: "suggestedContent" is REQUIRED (full file content). "diff" is optional.
- delete: only "path" is needed. The file will be removed.

IMPORTANT - Reliability first:
- For "modify" and "create": ALWAYS include suggestedContent with the complete file content. This ensures the apply works even when git patch fails.
- For "modify": ALSO provide a valid unified diff for preview. The diff MUST have proper headers (--- a/path, +++ b/path) and hunks (@@ -line,count +line,count @@).

Format rules:
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
   * Normaliza shape de alto nível para algo parecido com MissionPlan (steps array ou wrapper plan/missionPlan).
   */
  private coerceToMissionPlanLike(
    input: unknown,
    warnings: PlanRepairWarning[]
  ): { steps: unknown[]; summary?: string; estimatedComplexity?: string } | null {
    if (Array.isArray(input)) {
      warnings.push({ type: "NON_ARRAY_STEPS_COERCED" });
      return { steps: input };
    }
    if (input && typeof input === "object") {
      const obj = input as Record<string, unknown>;
      if (Array.isArray(obj.steps)) {
        return {
          steps: obj.steps,
          summary: typeof obj.summary === "string" ? obj.summary : undefined,
          estimatedComplexity:
            typeof obj.estimatedComplexity === "string"
              ? obj.estimatedComplexity
              : undefined,
        };
      }
      const plan = obj.plan as Record<string, unknown> | undefined;
      if (plan && Array.isArray(plan.steps)) {
        return {
          steps: plan.steps,
          summary:
            (typeof obj.summary === "string" ? obj.summary : undefined) ??
            (typeof plan.summary === "string" ? plan.summary : undefined),
          estimatedComplexity:
            typeof plan.estimatedComplexity === "string"
              ? plan.estimatedComplexity
              : undefined,
        };
      }
      const missionPlan = obj.missionPlan as Record<string, unknown> | undefined;
      if (missionPlan && Array.isArray(missionPlan.steps)) {
        return {
          steps: missionPlan.steps,
          summary:
            (typeof obj.summary === "string" ? obj.summary : undefined) ??
            (typeof missionPlan.summary === "string" ? missionPlan.summary : undefined),
          estimatedComplexity:
            typeof missionPlan.estimatedComplexity === "string"
              ? missionPlan.estimatedComplexity
              : undefined,
        };
      }
    }
    return null;
  }

  /**
   * Parse + reparo específico para MissionPlan. Retorna plan válido ou null e lista de warnings.
   * Se plan === null e há warning UNKNOWN_SHAPE, o chamador pode fazer retry com hint.
   */
  protected parseAndRepairPlan(
    raw: string
  ): { plan: MissionPlan | null; warnings: PlanRepairWarning[] } {
    const warnings: PlanRepairWarning[] = [];
    const trimmed = raw.trim();
    if (!trimmed) {
      return { plan: null, warnings: [{ type: "UNKNOWN_SHAPE" }] };
    }

    let parsed: unknown = null;
    try {
      const wrapper = JSON.parse(trimmed) as {
        result?: unknown;
        output?: unknown;
      };
      const inner = wrapper?.result ?? wrapper?.output;
      if (inner != null) {
        if (typeof inner === "string") {
          parsed = this.tryParseWithFixes(inner);
        } else if (typeof inner === "object") {
          parsed = inner;
        }
      }
    } catch {
      // not a wrapper
    }
    if (parsed == null) {
      parsed =
        this.parseJSONResponse<unknown>(trimmed) ?? this.tryParseWithFixes(trimmed);
    }
    if (parsed == null) {
      return { plan: null, warnings: [{ type: "UNKNOWN_SHAPE" }] };
    }

    const planLike = this.coerceToMissionPlanLike(parsed, warnings);
    if (!planLike || !Array.isArray(planLike.steps)) {
      return { plan: null, warnings: [...warnings, { type: "UNKNOWN_SHAPE" }] };
    }

    const validSteps: Array<Record<string, unknown> & { order: number }> = [];
    for (let index = 0; index < planLike.steps.length; index++) {
      const step = planLike.steps[index];
      if (!step || typeof step !== "object") {
        warnings.push({
          type: "STEP_MISSING_REQUIRED_FIELDS",
          stepIndex: index,
          missing: ["order", "title", "description"],
        });
        continue;
      }
      const stepObj = step as Record<string, unknown>;
      const missing: string[] = [];
      if (typeof stepObj.title !== "string") missing.push("title");
      if (typeof stepObj.description !== "string") missing.push("description");
      if (missing.length) {
        warnings.push({
          type: "STEP_MISSING_REQUIRED_FIELDS",
          stepIndex: index,
          missing,
        });
        continue;
      }
      let order = stepObj.order;
      if (
        typeof order === "string" &&
        order.trim() !== "" &&
        !Number.isNaN(Number(order))
      ) {
        warnings.push({
          type: "STEP_COERCED_TO_NUMBER",
          stepIndex: index,
          rawOrder: order,
        });
        order = Number(order) as number;
      }
      validSteps.push({ ...stepObj, order } as Record<string, unknown> & { order: number });
    }

    if (validSteps.length === 0) {
      warnings.push({ type: "EMPTY_STEPS_DROPPED" });
      return { plan: null, warnings };
    }

    const normalizedSteps = validSteps.map((step, index) => {
      let order = step.order;
      if (typeof order !== "number" || !Number.isFinite(order)) {
        warnings.push({ type: "STEP_MISSING_ORDER", stepIndex: index });
        order = index + 1;
      }
      let files = step.files;
      if (typeof files === "string") files = [files];
      if (!Array.isArray(files)) files = undefined;
      let status = step.status;
      if (
        typeof status !== "string" ||
        !["pending", "in_progress", "completed", "skipped"].includes(status)
      ) {
        status = undefined;
      }
      return {
        ...step,
        order,
        id: (step.id as string) ?? this.generateStepId(index),
        title: step.title as string,
        description: step.description as string,
        files,
        status,
      };
    });

    let complexity = planLike.estimatedComplexity;
    if (
      typeof complexity !== "string" ||
      !["low", "medium", "high"].includes(complexity)
    ) {
      complexity = undefined;
    }
    const summary =
      typeof planLike.summary === "string" ? planLike.summary : undefined;

    const candidate = {
      steps: normalizedSteps,
      summary,
      estimatedComplexity: complexity,
    };

    const result = missionPlanSchema.safeParse(candidate);
    if (!result.success) {
      return {
        plan: null,
        warnings: [
          ...warnings,
          {
            type: "UNKNOWN_SHAPE",
            detail: result.error?.message,
          },
        ],
      };
    }
    return { plan: result.data as MissionPlan, warnings };
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
   * Normaliza e valida resposta como MissionPlan via parseAndRepairPlan.
   * Em falha, retorna mensagem genérica e retryable quando o motivo for UNKNOWN_SHAPE.
   * Também extrai comandos pendentes detectados no plano.
   */
  protected parseAndValidateMissionPlan(
    raw: string
  ):
    | { success: true; data: MissionPlan; pendingCommands: PendingCommand[] }
    | { success: false; error: string; retryable?: boolean } {
    const { plan, warnings } = this.parseAndRepairPlan(raw);
    if (plan != null) {
      const pendingCommands = extractCommandsFromPlan(plan);
      return { success: true, data: plan, pendingCommands };
    }
    const retryable = warnings.some((w) => w.type === "UNKNOWN_SHAPE");
    return {
      success: false,
      error: PLAN_PARSE_ERROR_USER_MESSAGE,
      retryable,
    };
  }

  /**
   * Normaliza e valida resposta como GeneratedCode (com Zod).
   * Também extrai comandos pendentes detectados no código.
   */
  protected parseAndValidateGeneratedCode(
    raw: string
  ):
    | { success: true; data: GeneratedCode; pendingCommands: PendingCommand[] }
    | { success: false; error: string } {
    const result = this.normalizeAndValidate(raw, generatedCodeSchema);
    if (!result.success) return result;
    const code = result.data as GeneratedCode;
    const pendingCommands = extractCommandsFromCode(code);
    return { success: true, data: code, pendingCommands };
  }

  /**
   * Combina comandos pendentes de múltiplas fontes, removendo duplicatas.
   */
  protected mergePendingCommands(
    ...commandArrays: (PendingCommand[] | undefined)[]
  ): PendingCommand[] {
    const filtered = commandArrays.filter(
      (arr): arr is PendingCommand[] => arr !== undefined
    );
    return mergeCommands(...filtered);
  }

  /**
   * Inicia feedback de progresso baseado em timer para melhorar UX durante operações longas.
   * Retorna o ID do intervalo para poder cancelar depois.
   */
  protected startProgressFeedback(
    onProgress?: ProgressCallback,
    type: "plan" | "code" = "code"
  ): NodeJS.Timeout | null {
    if (!onProgress) return null;

    let step = 0;
    const messages =
      type === "plan"
        ? [
            "Analisando contexto do projeto...",
            "Identificando arquivos relevantes...",
            "Planejando estrutura de implementação...",
            "Definindo etapas do plano...",
            "Finalizando plano...",
          ]
        : [
            "Analisando contexto do projeto...",
            "Planejando alterações...",
            "Gerando sugestões de código...",
            "Finalizando resposta...",
          ];

    return setInterval(() => {
      if (step < messages.length) {
        onProgress(messages[step]);
        step++;
      }
    }, 8000); // A cada 8 segundos
  }
}
