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

export type PermissionMode = "" | "plan" | "acceptEdits" | "bypass";

export type MissionStatus =
  | "created"
  | "planning"
  | "plan_generated"
  | "generating_code"
  | "code_ready"
  | "applying"
  | "completed"
  | "failed"
  | "cancelled";

export interface MissionPlan {
  summary?: string;
  steps?: Array<Record<string, unknown>>;
  [key: string]: unknown;
}

export interface GeneratedCode {
  summary?: string;
  files?: Array<Record<string, unknown>>;
  [key: string]: unknown;
}

export interface CodeSuggestion {
  path: string;
  content: string;
  reason?: string;
}

export interface MissionLog {
  id: string;
  missionId: string;
  type: string;
  content: string;
  metadata?: Record<string, unknown> | null;
  createdAt: Date;
}

export interface Mission {
  id: string;
  projectId: string;
  providerId?: string | null;
  planProviderId?: string | null;
  codeProviderId?: string | null;
  title: string;
  description: string;
  status: MissionStatus;
  missionType?: string | null;
  plan?: MissionPlan | null;
  generatedCode?: GeneratedCode | null;
  context?: string | null;
  preserveInstructions?: string | null;
  errorMessage?: string | null;
  codeGenerationAttempts?: number;
  isCommitted?: boolean;
  isPushed?: boolean;
  pendingCommands?: PendingCommand[] | null;
  worktreePath?: string | null;
  worktreeBranch?: string | null;
  startedAt?: Date | null;
  completedAt?: Date | null;
  createdAt: Date;
  updatedAt: Date;
}

export interface MissionWithDetails extends Mission {
  logs?: MissionLog[];
}

export interface MissionLogStats {
  total: number;
  byLevel?: Record<string, number>;
  [key: string]: unknown;
}

export interface MissionCreate {
  projectId: string;
  providerId?: string | null;
  planProviderId?: string | null;
  codeProviderId?: string | null;
  title: string;
  description: string;
}

export interface MissionUpdate {
  title?: string;
  description?: string;
  providerId?: string | null;
  planProviderId?: string | null;
  codeProviderId?: string | null;
  status?: MissionStatus;
  plan?: MissionPlan | null;
  generatedCode?: GeneratedCode | null;
}

export interface MissionLogCreate {
  missionId: string;
  type: string;
  content: string;
  metadata?: Record<string, unknown> | null;
}

export interface PendingCommand {
  id: string;
  command: string;
  description?: string;
  source: "plan" | "code" | "file";
  confirmedAt: string | null;
}

export interface Project {
  id: string;
  name: string;
  path: string;
  description?: string | null;
  defaultProviderId?: string | null;
  gitRemoteUrl?: string | null;
  repoConfig?: ProjectRepoConfig | null;
  lastOpenedAt?: Date | null;
  createdAt: Date;
  updatedAt: Date;
}

export interface RepoProcessDefinition {
  id: string;
  name: string;
  command: string;
  description?: string | null;
  cwdMode?: "project" | "worktree";
  autoRestart?: boolean;
}

export interface RepoCommandPreset {
  id: string;
  name: string;
  command: string;
  description?: string | null;
}

export type RepoTaskTriggerWhen = "success" | "failure" | "complete";

export interface RepoTaskTriggerDefinition {
  when?: RepoTaskTriggerWhen;
  prompt?: string | null;
  providerId?: string | null;
}

export interface RepoTaskDefinition {
  id: string;
  name: string;
  command: string;
  schedule: string;
  description?: string | null;
  cwdMode?: "project" | "worktree";
  enabled?: boolean;
  trigger?: RepoTaskTriggerDefinition | null;
}

export interface ProjectRepoConfig {
  branchPrefix?: string | null;
  defaultAgentProviderId?: string | null;
  setupCommand?: string | null;
  teardownCommand?: string | null;
  processes?: RepoProcessDefinition[];
  presets?: RepoCommandPreset[];
  tasks?: RepoTaskDefinition[];
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
  repoConfig?: ProjectRepoConfig;
}

export interface UpdateProjectDTO {
  name?: string;
  description?: string;
  defaultProviderId?: string;
  gitRemoteUrl?: string;
  repoConfig?: ProjectRepoConfig | null;
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

export interface ProjectsQueryOptions {
  limit?: number;
  offset?: number;
  orderBy?: 'name' | 'lastOpenedAt' | 'createdAt';
  orderDirection?: 'asc' | 'desc';
}

export interface PanesQueryOptions {
  limit?: number;
  offset?: number;
  combId?: string;
  type?: PaneType | PaneType[];
  status?: PaneStatus | PaneStatus[];
  orderBy?: 'layoutOrder' | 'createdAt' | 'updatedAt' | 'lastActivityAt';
  orderDirection?: 'asc' | 'desc';
}

export interface PaneSession {
  ptyId?: string;
  cwd?: string;
  command?: string;
  args?: string[];
  status?: 'running' | 'exited';
  startedAt?: string;
  exitedAt?: string | null;
  lastExitCode?: number | null;
  outputPreview?: string;
}

export interface MissionAgentSession extends PaneSession {
  missionId?: string;
}

export interface ProjectStats {
  totalWorkspaces: number;
  appliedWorkspaces: number;
  activeWorkspaces: number;
  failedWorkspaces: number;
}

// Aliases para compatibilidade com tipos antigos
export type ProviderCreate = CreateProviderDTO;
export type ProviderUpdate = UpdateProviderDTO;
export type ProjectCreate = CreateProjectDTO;
export type ProjectUpdate = UpdateProjectDTO;
export type CombCreate = CreateCombDTO;
export type CombUpdate = UpdateCombDTO;
export type PaneCreate = CreatePaneDTO;
export type PaneUpdate = UpdatePaneDTO;
