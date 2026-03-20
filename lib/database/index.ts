// Dev Command Center - Database Module
// Ponto de entrada para toda a camada de persistência SQLite

// Conexão e utilitários
export {
  initDatabase,
  getDatabase,
  closeDatabase,
  isDatabaseConnected,
  withTransaction,
  getAppDataPath,
  getDatabasePath,
  generateId,
  setUserDataPath,
} from "./connection";

// Repositórios
export { ProvidersRepository } from "./repositories/providers";
export { ProjectsRepository } from "./repositories/projects";
export { MissionsRepository } from "./repositories/missions";
export { MissionLogsRepository } from "./repositories/mission-logs";
export { CombsRepository } from "./repositories/combs";
export { PanesRepository } from "./repositories/panes";

// Tipos
export type {
  // Enums e tipos base
  ProviderType,
  MissionStatus,
  MissionType,
  MissionLogType,
  CombStatus,
  PaneType,
  PaneStatus,

  // Entidades
  Provider,
  ProviderConfig,
  Project,
  Mission,
  MissionPlan,
  PlanStep,
  GeneratedCode,
  CodeSuggestion,
  MissionContext,
  MissionLog,
  MissionLogMetadata,
  Comb,
  Pane,
  PaneSession,

  // DTOs
  CreateProviderDTO,
  UpdateProviderDTO,
  CreateProjectDTO,
  UpdateProjectDTO,
  CreateMissionDTO,
  UpdateMissionDTO,
  CreateMissionLogDTO,
  CreateCombDTO,
  UpdateCombDTO,
  CreatePaneDTO,
  UpdatePaneDTO,

  // Query options
  PaginationOptions,
  ProjectsQueryOptions,
  MissionsQueryOptions,
  MissionLogsQueryOptions,
  CombsQueryOptions,
  PanesQueryOptions,
} from "./types";

// ============================================
// Funções de conveniência para uso no Electron
// ============================================

import {
  initDatabase,
  closeDatabase,
  getDatabasePath,
  isDatabaseConnected,
} from "./connection";
import { ProvidersRepository } from "./repositories/providers";
import { ProjectsRepository } from "./repositories/projects";
import { MissionsRepository } from "./repositories/missions";
import { MissionLogsRepository } from "./repositories/mission-logs";
import { CombsRepository } from "./repositories/combs";
import { PanesRepository } from "./repositories/panes";

/**
 * Objeto DB para acesso simplificado aos repositórios.
 * Uso: db.providers.findAll(), db.projects.create(...), etc.
 */
export const db = {
  providers: ProvidersRepository,
  projects: ProjectsRepository,
  missions: MissionsRepository,
  missionLogs: MissionLogsRepository,
  combs: CombsRepository,
  panes: PanesRepository,

  /**
   * Inicializa o banco de dados.
   * Deve ser chamado uma vez na inicialização do app Electron.
   */
  init: initDatabase,

  /**
   * Fecha a conexão com o banco.
   * Deve ser chamado ao fechar o app Electron.
   */
  close: closeDatabase,

  /**
   * Obtém o caminho do banco de dados.
   */
  getPath: (): string | null => {
    if (!isDatabaseConnected()) {
      return null;
    }
    return getDatabasePath();
  },
};

// Default export para uso mais limpo
export default db;
