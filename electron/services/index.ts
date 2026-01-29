/**
 * Electron Services - Exporta todos os serviços
 */

// Types
export type {
  AIProviderAdapter,
  AIResponse,
  AIResponseMetadata,
  AdapterConfig,
  ProjectContext,
  GitInfo,
  GitStatus,
  GitCommit,
  ValidationResult,
  ApplyChangesOptions,
  ApplyChangesResult,
  StreamCallback,
} from "./types";

// Git Service
export { GitService, createGitService } from "./git-service";

// Adapters
export {
  BaseAdapter,
  ClaudeCodeAdapter,
  CodexAdapter,
  OpenAIAdapter,
  AnthropicAdapter,
  createAdapter,
  createClaudeCodeAdapter,
  createCodexAdapter,
  createOpenAIAdapter,
  createAnthropicAdapter,
  adapterRegistry,
} from "./adapters";

// AI Orchestrator
export {
  AIOrchestrator,
  aiOrchestrator,
  createAIOrchestrator,
} from "./ai-orchestrator";
