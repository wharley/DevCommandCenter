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

/** Status simplificado para o Wall (agents_cli). */
export type WallStatus =
  | "running"
  | "ready_for_review"
  | "applied"
  | "discarded"
  | "canceled"
  | "error"
  | "apply_failed";

/** Resumo Git para o rodapé do card (agents_cli). */
export interface LastGitSummary {
  changedFiles: number;
  insertions: number;
  deletions: number;
}

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
  /** Branch de origem ao criar worktree (agents_cli) */
  baseBranch?: string | null;
  /** Branch de destino após aplicar patch (agents_cli) */
  targetBranch?: string | null;
  /** Resumo da saída do terminal para o card (agents_cli) */
  lastOutputSummary?: string | null;
  /** Resumo Git para o rodapé do card (agents_cli) */
  lastGitSummary?: LastGitSummary | null;
  /** Status simplificado para o Wall (agents_cli) */
  wallStatus?: WallStatus | null;
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
  completionSummary?: string;
  agentSession?: MissionAgentSession | null;
  gitSnapshot?: MissionGitSnapshot | null;
}

export interface MissionAgentSession {
  ptyId?: string | null;
  cwd?: string;
  command?: string | null;
  args?: string[];
  status?: "idle" | "running" | "exited";
  startedAt?: string;
  lastActivityAt?: string;
  exitedAt?: string | null;
  lastExitCode?: number | null;
  outputPreview?: string;
  outputLineCount?: number;
}

export interface MissionGitSnapshot {
  branch?: string;
  upstreamBranch?: string | null;
  defaultBranch?: string | null;
  isRepo: boolean;
  isDirty: boolean;
  changedFiles?: string[];
  stagedCount?: number;
  unstagedCount?: number;
  untrackedCount?: number;
  aheadCount?: number;
  behindCount?: number;
  aheadOfDefaultCount?: number;
  behindOfDefaultCount?: number;
  hasUpstream?: boolean;
  mergeReadiness?:
    | "ready"
    | "dirty"
    | "behind_default"
    | "diverged"
    | "already_merged"
    | "not_applicable";
}

// ============================================
// Comb & Pane (Hive/Comb/Pane architecture)
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

/**
 * Target genérico de revisão multi-repo na mesma Missão (Comb).
 * O target primário usa a worktree da Missão (`sourceCombId` = id do comb).
 * Targets extras referenciam outros projetos (checkout local em `project.path`).
 */
export interface ReviewTarget {
  id: string;
  label: string;
  projectId: string;
  /** Quando igual ao id do comb atual, diffs/patch usam a worktree da Missão. */
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
  /** JSON: repos adicionais para revisão na mesma aba (além do projeto primário). */
  reviewTargets?: ReviewTarget[] | null;
  status: CombStatus;
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

export interface PaneSession {
  ptyId?: string | null;
  cwd?: string;
  command?: string | null;
  args?: string[];
  status?: "idle" | "running" | "exited";
  startedAt?: string;
  lastActivityAt?: string;
  exitedAt?: string | null;
  lastExitCode?: number | null;
  outputPreview?: string;
  outputLineCount?: number;
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
  /** Interno: blob criptografado (host encripta antes de chamar repo) */
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
  /** Branch de origem para worktree (agents_cli) */
  baseBranch?: string | null;
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
  errorMessage?: string | null;
  codeGenerationAttempts?: number;
  isCommitted?: boolean;
  isPushed?: boolean;
  pendingCommands?: PendingCommand[] | null;
  worktreePath?: string | null;
  worktreeBranch?: string | null;
  baseBranch?: string | null;
  targetBranch?: string | null;
  lastOutputSummary?: string | null;
  lastGitSummary?: LastGitSummary | null;
  wallStatus?: WallStatus | null;
  startedAt?: Date | null;
  completedAt?: Date | null;
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

export interface CombsQueryOptions extends PaginationOptions {
  projectId?: string;
  status?: CombStatus | CombStatus[];
  orderBy?: "name" | "status" | "createdAt" | "updatedAt" | "lastOpenedAt";
  orderDirection?: "asc" | "desc";
}

export interface PanesQueryOptions extends PaginationOptions {
  combId?: string;
  type?: PaneType;
  orderBy?: "layoutOrder" | "createdAt" | "lastActivityAt";
  orderDirection?: "asc" | "desc";
}

// ============================================
// Aliases para compatibilidade (types/app.d.ts, etc.)
// ============================================

export type ProjectCreate = CreateProjectDTO;
export type ProjectUpdate = UpdateProjectDTO;
export type ProviderCreate = CreateProviderDTO;
export type ProviderUpdate = UpdateProviderDTO;
export type MissionCreate = CreateMissionDTO;
export type MissionUpdate = UpdateMissionDTO;
export type MissionLogCreate = CreateMissionLogDTO;
export type CombCreate = CreateCombDTO;
export type CombUpdate = UpdateCombDTO;
export type PaneCreate = CreatePaneDTO;
export type PaneUpdate = UpdatePaneDTO;

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
