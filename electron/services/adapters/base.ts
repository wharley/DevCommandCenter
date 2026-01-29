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
} from "../types";

export abstract class BaseAdapter implements AIProviderAdapter {
  abstract readonly name: string;
  abstract readonly type: Provider["type"];

  protected provider: Provider;

  constructor(provider: Provider) {
    this.provider = provider;
  }

  abstract validate(): ValidationResult;

  abstract generatePlan(config: AdapterConfig): Promise<AIResponse<MissionPlan>>;

  abstract generateCode(config: AdapterConfig): Promise<AIResponse<GeneratedCode>>;

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
      "diff": "unified diff format (optional)"
    }
  ]
}

Important:
- Use proper indentation and formatting
- Follow the project's existing code style
- Include all necessary imports
- Respond ONLY with valid JSON. Do not include any other text or markdown code blocks.`;
  }

  /**
   * Tenta fazer parse de JSON de uma resposta que pode ter texto adicional
   */
  protected parseJSONResponse<T>(response: string): T | null {
    // Tenta parse direto primeiro
    try {
      return JSON.parse(response) as T;
    } catch {
      // Ignora e tenta extrair JSON
    }

    // Tenta extrair JSON de blocos de código markdown
    const codeBlockMatch = response.match(/```(?:json)?\s*([\s\S]*?)```/);
    if (codeBlockMatch) {
      try {
        return JSON.parse(codeBlockMatch[1].trim()) as T;
      } catch {
        // Ignora e continua
      }
    }

    // Tenta encontrar JSON no texto
    const jsonMatch = response.match(/\{[\s\S]*\}/);
    if (jsonMatch) {
      try {
        return JSON.parse(jsonMatch[0]) as T;
      } catch {
        // Ignora
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
