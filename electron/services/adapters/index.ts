/**
 * Adapters Index - Exporta todos os adapters e factory
 */

export { BaseAdapter } from "./base";
export { ClaudeCodeAdapter, createClaudeCodeAdapter } from "./claude-code";
export { CodexAdapter, createCodexAdapter } from "./codex";
export { OpenAIAdapter, createOpenAIAdapter } from "./openai";
export { AnthropicAdapter, createAnthropicAdapter } from "./anthropic";
export { CursorAdapter, createCursorAdapter } from "./cursor";

import type { AIProviderAdapter, Provider } from "../types";
import { ClaudeCodeAdapter } from "./claude-code";
import { CodexAdapter } from "./codex";
import { OpenAIAdapter } from "./openai";
import { AnthropicAdapter } from "./anthropic";
import { CursorAdapter } from "./cursor";

/**
 * Factory para criar o adapter correto baseado no tipo do provider
 */
export function createAdapter(provider: Provider): AIProviderAdapter {
  switch (provider.type) {
    case "claude-code":
      return new ClaudeCodeAdapter(provider);
    case "codex":
      return new CodexAdapter(provider);
    case "openai":
      return new OpenAIAdapter(provider);
    case "anthropic":
      return new AnthropicAdapter(provider);
    case "cursor":
      return new CursorAdapter(provider);
    case "custom":
      // Para custom, tenta detectar o melhor adapter baseado na config
      if (provider.cliPath) {
        // Se tem CLI path, assume que é um CLI
        if (provider.cliPath.includes("claude")) {
          return new ClaudeCodeAdapter(provider);
        } else         if (provider.cliPath.includes("codex")) {
          return new CodexAdapter(provider);
        }
        if (provider.cliPath.includes("agent") || provider.cliPath.includes("cursor")) {
          return new CursorAdapter(provider);
        }
      }
      if (provider.apiKey) {
        // Se tem API key, tenta detectar pelo formato
        if (provider.apiKey.startsWith("sk-ant-")) {
          return new AnthropicAdapter(provider);
        }
        // Fallback para OpenAI
        return new OpenAIAdapter(provider);
      }
      throw new Error(`Cannot determine adapter for custom provider: ${provider.name}`);
    default:
      throw new Error(`Unknown provider type: ${provider.type}`);
  }
}

/**
 * Registry de adapters disponíveis
 */
export const adapterRegistry = {
  "claude-code": {
    name: "Claude Code CLI",
    description: "Uses Claude Code CLI installed locally",
    requiresCli: true,
    requiresApiKey: false,
  },
  "codex": {
    name: "OpenAI Codex CLI",
    description: "Uses OpenAI Codex CLI installed locally",
    requiresCli: true,
    requiresApiKey: true,
  },
  "openai": {
    name: "OpenAI API",
    description: "Direct integration with OpenAI API (GPT-4, etc.)",
    requiresCli: false,
    requiresApiKey: true,
  },
  "anthropic": {
    name: "Anthropic API",
    description: "Direct integration with Anthropic API (Claude)",
    requiresCli: false,
    requiresApiKey: true,
  },
  "cursor": {
    name: "Cursor Agent CLI",
    description: "Uses Cursor Agent CLI (agent) installed locally. Login is done in the terminal.",
    requiresCli: true,
    requiresApiKey: false,
  },
  "custom": {
    name: "Custom Provider",
    description: "Custom provider configuration",
    requiresCli: false,
    requiresApiKey: false,
  },
} as const;
