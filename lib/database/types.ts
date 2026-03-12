// Dev Command Center - Database Types
// Tipos TypeScript para as entidades do banco de dados

// ============================================
// Enums e tipos base
// ============================================

// Deve corresponder ao CHECK constraint em schema.sql
export type ProviderType =
  | "claude-code"
  | "codex"
  | "openai"
  | "anthropic"
  | "cursor"
  | "gemini"
  | "custom";

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

export type MissionLogType =
  | "info"
  | "prompt"
  | "response"
  | "error"
  | "action"
  | "user_input"
  | "warning"
  | "debug";

/** implementation = plano → código → aplicar; analysis = apenas plano; agents_cli = tarefa 1:1 com agente no terminal */
export type MissionType = "implementation" | "analysis" | "agents_cli";

/**
 * Permission mode for CLI agents (Codex, Claude Code, etc.).
 * - plan: only plan, require approval for edits
 * - acceptEdits: auto-accept edits (e.g. --full-auto for Codex)
 * - bypass: skip approvals and sandbox (maximum automation)
 */
export type PermissionMode = "" | "plan" | "acceptEdits" | "bypass";

// ============================================
// Entidades do banco
// ============================================

export interface Provider {
  id: string;
  name: string;
  type: ProviderType;
  apiKey?: string | null;
  /** Interno: blob criptografado (para hydration no Electron) */
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
  permissionMode?: PermissionMode | null;
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

export interface Mission {
  id: string;
  projectId: string;
  providerId?: string | null;
  planProviderId?: string | null;
  codeProviderId?: string | null;
  title: string;
  description: string;
  status: MissionStatus;
  /** implementation = plano → código → aplicar; analysis = apenas plano. Default implementation. */
  missionType?: MissionType | null;
  plan?: MissionPlan | null;
  generatedCode?: GeneratedCode | null;
  context?: MissionContext | null;
  preserveInstructions?: string | null;
  errorMessage?: string | null;
  codeGenerationAttempts?: number;
  isCommitted?: boolean;
  isPushed?: boolean;
  /** Comandos pendentes detectados que o usuário precisa executar manualmente */
  pendingCommands?: PendingCommand[] | null;
  /** Path do worktree Git associado à missão (para pipeline paralelo) */
  worktreePath?: string | null;
  /** Branch do worktree (ex.: dcc-mission-<id>) */
  worktreeBranch?: string | null;
  startedAt?: Date | null;
  completedAt?: Date | null;
  createdAt: Date;
  updatedAt: Date;
}

export interface MissionPlan {
  steps: PlanStep[];
  summary?: string;
  estimatedComplexity?: "low" | "medium" | "high";
}

export interface PlanStep {
  id: string;
  order: number;
  title: string;
  description: string;
  files?: string[];
  status?: "pending" | "in_progress" | "completed" | "skipped";
}

export interface GeneratedCode {
  files: CodeSuggestion[];
  summary?: string;
}

export interface CodeSuggestion {
  path: string;
  action: "create" | "modify" | "delete";
  originalContent?: string;
  suggestedContent?: string;
  diff?: string;
}

export interface MissionContext {
  files: string[];
  fileContents?: Record<string, string>;
  gitBranch?: string;
  gitStatus?: string;
}

/** Comando pendente detectado que o usuário precisa executar manualmente */
export interface PendingCommand {
  id: string;
  command: string;
  description?: string;
  /** Onde o comando foi detectado: plan, code ou file (ex: package.json) */
  source: "plan" | "code" | "file";
  /** Data/hora em que o usuário confirmou ter executado o comando */
  confirmedAt?: Date | null;
}

export interface MissionLog {
  id: string;
  missionId: string;
  type: MissionLogType;
  content: string;
  metadata?: MissionLogMetadata | null;
  createdAt: Date;
}

export interface MissionLogMetadata {
  tokensUsed?: number;
  durationMs?: number;
  model?: string;
  [key: string]: unknown;
}

// ============================================
// DTOs para criação/atualização
// ============================================

export interface CreateProviderDTO {
  name: string;
  type: ProviderType;
  apiKey?: string;
  /** Interno: blob criptografado (Electron encripta antes de chamar repo) */
  apiKeyEncrypted?: Buffer;
  cliPath?: string;
  config?: ProviderConfig;
  isActive?: boolean;
}

export interface UpdateProviderDTO {
  name?: string;
  type?: ProviderType;
  apiKey?: string | null;
  /** Interno: blob criptografado */
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

export interface CreateMissionDTO {
  projectId: string;
  providerId?: string;
  planProviderId?: string;
  codeProviderId?: string;
  title: string;
  description: string;
  preserveInstructions?: string;
  missionType?: MissionType;
}

export interface UpdateMissionDTO {
  title?: string;
  description?: string;
  providerId?: string;
  planProviderId?: string | null;
  codeProviderId?: string | null;
  preserveInstructions?: string | null;
  missionType?: MissionType | null;
  status?: MissionStatus;
  plan?: MissionPlan;
  generatedCode?: GeneratedCode;
  context?: MissionContext;
  errorMessage?: string;
  codeGenerationAttempts?: number;
  isCommitted?: boolean;
  isPushed?: boolean;
  pendingCommands?: PendingCommand[] | null;
  worktreePath?: string | null;
  worktreeBranch?: string | null;
  startedAt?: Date;
  completedAt?: Date;
}

export interface CreateMissionLogDTO {
  missionId: string;
  type: MissionLogType;
  content: string;
  metadata?: MissionLogMetadata;
}

// ============================================
// Tipos para queries
// ============================================

export interface PaginationOptions {
  limit?: number;
  offset?: number;
}

export interface ProjectsQueryOptions extends PaginationOptions {
  orderBy?: "name" | "lastOpenedAt" | "createdAt";
  orderDirection?: "asc" | "desc";
}

export interface MissionsQueryOptions extends PaginationOptions {
  projectId?: string;
  status?: MissionStatus | MissionStatus[];
  orderBy?: "title" | "status" | "createdAt" | "updatedAt";
  orderDirection?: "asc" | "desc";
}

export interface MissionLogsQueryOptions extends PaginationOptions {
  missionId?: string;
  type?: MissionLogType | MissionLogType[];
}

// ============================================
// Aliases para compatibilidade (electron.d.ts, etc.)
// ============================================

export type ProjectCreate = CreateProjectDTO;
export type ProjectUpdate = UpdateProjectDTO;
export type ProviderCreate = CreateProviderDTO;
export type ProviderUpdate = UpdateProviderDTO;
export type MissionCreate = CreateMissionDTO;
export type MissionUpdate = UpdateMissionDTO;
export type MissionLogCreate = CreateMissionLogDTO;

/** Stats for a project (mission counts). */
export interface ProjectStats {
  totalMissions: number;
  completedMissions: number;
  activeMissions: number;
  failedMissions: number;
}

/** Mission with related logs. */
export interface MissionWithDetails {
  mission: Mission;
  logs: MissionLog[];
}

/** Mission log level (alias for MissionLogType). */
export type LogLevel = MissionLogType;

/** Stats for mission logs. */
export interface MissionLogStats {
  total: number;
  byType: Record<MissionLogType, number>;
}
