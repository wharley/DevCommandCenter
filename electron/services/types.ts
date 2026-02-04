/**
 * Tipos compartilhados para os serviços de IA do Electron
 */

import type {
  Provider,
  Mission,
  MissionPlan,
  GeneratedCode,
  PlanStep,
  CodeSuggestion,
} from "../../lib/database/types";

// ============================================
// Tipos de Resposta da IA
// ============================================

export interface AIResponse<T = MissionPlan | GeneratedCode> {
  success: boolean;
  data?: T;
  error?: string;
  metadata?: AIResponseMetadata;
}

export interface AIResponseMetadata {
  tokensUsed?: number;
  durationMs?: number;
  model?: string;
  provider?: string;
}

// ============================================
// Contexto do Projeto para IA
// ============================================

export interface ProjectContext {
  projectPath: string;
  projectName: string;
  files: string[];
  fileContents?: Record<string, string>;
  gitInfo?: GitInfo;
}

export interface GitInfo {
  branch: string;
  remote?: string;
  status: GitStatus;
  recentCommits?: GitCommit[];
}

export interface GitStatus {
  isRepo: boolean;
  isDirty: boolean;
  staged: string[];
  unstaged: string[];
  untracked: string[];
}

export interface GitCommit {
  hash: string;
  message: string;
  author: string;
  date: Date;
}

// ============================================
// Configuração dos Adapters
// ============================================

export interface AdapterConfig {
  provider: Provider;
  mission: Mission;
  projectContext: ProjectContext;
  /** Feedback do usuário ao regenerar plano (o que ajustar no plano anterior) */
  planFeedback?: string;
  /** Feedback do usuário ao regenerar código (o que deu errado na tentativa anterior) */
  codeFeedback?: string;
}

// ============================================
// Interface do Adapter (contrato)
// ============================================

export interface AIProviderAdapter {
  readonly name: string;
  readonly type: Provider["type"];

  /**
   * Valida se o provider está configurado corretamente
   */
  validate(): ValidationResult;

  /**
   * Gera um plano de ação para a missão
   * @param config Configuração do adapter (provider, mission, projectContext)
   * @param onProgress Callback opcional para notificar progresso
   */
  generatePlan(
    config: AdapterConfig,
    onProgress?: ProgressCallback
  ): Promise<AIResponse<MissionPlan>>;

  /**
   * Gera sugestões de código baseadas no plano
   * @param config Configuração do adapter (provider, mission, projectContext)
   * @param onProgress Callback opcional para notificar progresso
   */
  generateCode(
    config: AdapterConfig,
    onProgress?: ProgressCallback
  ): Promise<AIResponse<GeneratedCode>>;

  /**
   * Executa um comando/prompt direto (para interação em tempo real)
   */
  execute?(prompt: string, config: AdapterConfig): Promise<AIResponse<string>>;

  /**
   * Verifica se a conexão com o provider está funcionando
   */
  testConnection(): Promise<{ success: boolean; message: string }>;
}

export interface ValidationResult {
  valid: boolean;
  errors: string[];
  warnings?: string[];
}

// ============================================
// Tipos para Streaming e Progresso
// ============================================

export interface StreamCallback {
  onData: (chunk: string) => void;
  onError: (error: Error) => void;
  onComplete: () => void;
}

/**
 * Callback para notificar progresso durante operações de IA
 * Recebe uma mensagem de status que pode ser exibida ao usuário
 */
export type ProgressCallback = (message: string) => void;

// ============================================
// Tipos para Aplicação de Mudanças
// ============================================

export interface ApplyChangesOptions {
  projectPath: string;
  changes: CodeSuggestion[];
  createBackup?: boolean;
  dryRun?: boolean;
}

export interface ApplyChangesResult {
  success: boolean;
  appliedFiles: string[];
  failedFiles: Array<{ path: string; error: string }>;
  backupPath?: string;
  /** How each file was applied (git-apply, file-write, or already-applied) for transparency */
  appliedVia?: Array<{
    path: string;
    via: "git-apply" | "file-write" | "already-applied";
  }>;
}

// Re-exports para conveniência
export type {
  Provider,
  Mission,
  MissionPlan,
  GeneratedCode,
  PlanStep,
  CodeSuggestion,
};
