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

// ============================================
// Entidades do banco
// ============================================

export interface Provider {
  id: string;
  name: string;
  type: ProviderType;
  apiKey?: string | null;
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
  title: string;
  description: string;
  status: MissionStatus;
  plan?: MissionPlan | null;
  generatedCode?: GeneratedCode | null;
  context?: MissionContext | null;
  errorMessage?: string | null;
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
  cliPath?: string;
  config?: ProviderConfig;
  isActive?: boolean;
}

export interface UpdateProviderDTO {
  name?: string;
  type?: ProviderType;
  apiKey?: string;
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
  title: string;
  description: string;
}

export interface UpdateMissionDTO {
  title?: string;
  description?: string;
  providerId?: string;
  status?: MissionStatus;
  plan?: MissionPlan;
  generatedCode?: GeneratedCode;
  context?: MissionContext;
  errorMessage?: string;
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
