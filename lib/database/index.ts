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

// Tipos
export type {
  // Enums e tipos base
  ProviderType,
  MissionStatus,
  MissionType,
  MissionLogType,

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

  // DTOs
  CreateProviderDTO,
  UpdateProviderDTO,
  CreateProjectDTO,
  UpdateProjectDTO,
  CreateMissionDTO,
  UpdateMissionDTO,
  CreateMissionLogDTO,

  // Query options
  PaginationOptions,
  ProjectsQueryOptions,
  MissionsQueryOptions,
  MissionLogsQueryOptions,
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

/**
 * Objeto DB para acesso simplificado aos repositórios.
 * Uso: db.providers.findAll(), db.projects.create(...), etc.
 */
export const db = {
  providers: ProvidersRepository,
  projects: ProjectsRepository,
  missions: MissionsRepository,
  missionLogs: MissionLogsRepository,

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
