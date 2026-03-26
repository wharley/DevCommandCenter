// Dev Command Center - Database Types
// Tipos TypeScript para as entidades do banco de dados

// ============================================
// Enums e tipos base
// ============================================

export type ProviderType =
  | "claude-code"
  | "codex"
  | "openai"
  | "anthropic"
  | "cursor"
  | "gemini"
  | "custom";

export interface Provider {
  id: string;
  name: string;
  type: ProviderType;
  apiKey?: string | null;
  /** Interno: blob criptografado (para hydration no host) */
  apiKeyEncrypted?: Buffer | null;
  /** Para UI: true quando há key armazenada (sem expor o valor) */
  hasApiKey?: boolean;
  cliPath?: string | null;
  config?: ProviderConfig | null;
  isActive: boolean;
  createdAt: Date;
  updatedAt: Date;
}

export interface ProviderConfig {
  model?: string;
  maxTokens?: number;
  temperature?: number;
  baseUrl?: string;
  /** Timeout máximo em milissegundos para operações CLI (padrão: 10 minutos) */
  timeout?: number;
  /** Permission mode for CLI agents: plan (approval required), acceptEdits (auto-accept), bypass (skip approvals) */
  permissionMode?: string | null;
  [key: string]: unknown;
}

export interface Project {
  id: string;
  name: string;
  path: string;
  description?: string | null;
  defaultProviderId?: string | null;
  gitRemoteUrl?: string | null;
  lastOpenedAt?: Date | null;
  createdAt: Date;
  updatedAt: Date;
}

// ============================================
// Workspace & Pane (Hive architecture)
// ============================================

export type CombStatus =
  | "active"
  | "ready_for_review"
  | "applied"
  | "discarded"
  | "archived"
  | "error";

export type PaneType = "term" | "agent";

export type PaneStatus = "idle" | "running" | "exited";

export interface ReviewTarget {
  id: string;
  label: string;
  projectId: string;
  sourceCombId?: string | null;
}

export interface Comb {
  id: string;
  projectId: string;
  name: string;
  description?: string | null;
  baseBranch: string;
  branch?: string | null;
  worktreePath?: string | null;
  reviewTargets?: ReviewTarget[] | null;
  status: CombStatus;
  isPinned?: boolean;
  pinnedAt?: Date | null;
  lastOpenedAt?: Date | null;
  createdAt: Date;
  updatedAt: Date;
}

export interface Pane {
  id: string;
  combId: string;
  type: PaneType;
  providerId?: string | null;
  title?: string | null;
  initialPrompt?: string | null;
  cwd?: string | null;
  ptyOwnerKey?: string | null;
  status: PaneStatus;
  layoutOrder: number;
  lastActivityAt?: Date | null;
  createdAt: Date;
  updatedAt: Date;
}

// ============================================
// DTOs
// ============================================

export interface CreateProviderDTO {
  name: string;
  type: ProviderType;
  apiKey?: string;
  apiKeyEncrypted?: Buffer;
  cliPath?: string;
  config?: ProviderConfig;
  isActive?: boolean;
}

export interface UpdateProviderDTO {
  name?: string;
  type?: ProviderType;
  apiKey?: string | null;
  apiKeyEncrypted?: Buffer;
  cliPath?: string;
  config?: ProviderConfig;
  isActive?: boolean;
}

export interface CreateProjectDTO {
  name: string;
  path: string;
  description?: string;
  defaultProviderId?: string;
  gitRemoteUrl?: string;
}

export interface UpdateProjectDTO {
  name?: string;
  description?: string;
  defaultProviderId?: string;
  gitRemoteUrl?: string;
  lastOpenedAt?: Date;
}

export interface CreateCombDTO {
  projectId: string;
  name: string;
  description?: string;
  baseBranch: string;
}

export interface UpdateCombDTO {
  name?: string;
  description?: string | null;
  branch?: string | null;
  worktreePath?: string | null;
  reviewTargets?: ReviewTarget[] | null;
  status?: CombStatus;
  isPinned?: boolean;
  pinnedAt?: Date | null;
  lastOpenedAt?: Date;
}

export interface CreatePaneDTO {
  combId: string;
  type: PaneType;
  providerId?: string;
  title?: string;
  initialPrompt?: string;
  layoutOrder?: number;
}

export interface UpdatePaneDTO {
  title?: string | null;
  initialPrompt?: string | null;
  providerId?: string | null;
  cwd?: string | null;
  ptyOwnerKey?: string | null;
  status?: PaneStatus;
  layoutOrder?: number;
  lastActivityAt?: Date;
}

export interface PaginationOptions {
  limit?: number;
  offset?: number;
}

export interface CombsQueryOptions {
  limit?: number;
  offset?: number;
  projectId?: string;
  status?: CombStatus | CombStatus[];
  orderBy?: 'name' | 'status' | 'createdAt' | 'updatedAt' | 'lastOpenedAt';
  orderDirection?: 'asc' | 'desc';
}

// Aliases para compatibilidade com tipos antigos
export type CombCreate = CreateCombDTO;
export type CombUpdate = UpdateCombDTO;
export type PaneCreate = CreatePaneDTO;
export type PaneUpdate = UpdatePaneDTO;
