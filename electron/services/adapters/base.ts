/**
 * Base Adapter - Classe base abstrata para adapters de IA
 */

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
    onProgress?: ProgressCallback,
  ): Promise<AIResponse<MissionPlan>>;

  abstract generateCode(
    config: AdapterConfig,
    onProgress?: ProgressCallback,
  ): Promise<AIResponse<GeneratedCode>>;

  abstract testConnection(): Promise<{ success: boolean; message: string }>;

  /**
   * Monta o prompt base para geração de plano
   */
  protected buildPlanPrompt(config: AdapterConfig): string {
    const { mission, projectContext } = config;

    return `You are an expert software engineer. Analyze the following task and create a detailed implementation plan.

## Project Context
- **Project Name**: ${projectContext.projectName}
- **Project Path**: ${projectContext.projectPath}
${projectContext.gitInfo ? `- **Git Branch**: ${projectContext.gitInfo.branch}` : ""}

## Available Files
${projectContext.files.slice(0, 50).join("\n")}
${projectContext.files.length > 50 ? `\n... and ${projectContext.files.length - 50} more files` : ""}

## Task
**Title**: ${mission.title}

**Description**: 
${mission.description}

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

## Instructions
Generate the code changes with the following JSON structure:
{
  "summary": "Summary of changes made",
  "files": [
    {
      "path": "relative/path/to/file.ts",
      "action": "create" | "modify" | "delete",
      "originalContent": "original content if modifying (optional)",
      "suggestedContent": "the new/modified content",
      "diff": "unified diff only: lines starting with +, -, ---, +++ or space for context. If no code changes are needed for this file, omit diff or set to empty string. Do NOT put explanatory text or comments in diff."
    }
  ]
}

Important:
- Use proper indentation and formatting
- Follow the project's existing code style
- Include all necessary imports
- The "diff" field must contain ONLY a unified diff (lines with +, -, ---, +++). If there are no changes for a file, leave diff empty or omit it; do not put messages like "No further changes needed" in diff.
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

    // 2. Bloco markdown: conteúdo após ```json ou ``` (pode conter backticks no conteúdo)
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
    }

    // 3. Extração por contagem de chaves no texto inteiro
    const byBrace = this.extractTopLevelJson(trimmed);
    if (byBrace) {
      try {
        return JSON.parse(byBrace) as T;
      } catch {
        // segue
      }
    }

    // 4. Regex guloso como último recurso
    const jsonMatch = trimmed.match(/\{[\s\S]*\}/);
    if (jsonMatch) {
      try {
        return JSON.parse(jsonMatch[0]) as T;
      } catch {
        // segue
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
}
