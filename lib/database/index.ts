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
export { CombsRepository } from "./repositories/combs";
export { PanesRepository } from "./repositories/panes";

// Tipos
export type {
  ProviderType,
  CombStatus,
  PaneType,
  PaneStatus,

  Provider,
  ProviderConfig,
  Project,
  Comb,
  Pane,
  PaneSession,

  CreateProviderDTO,
  UpdateProviderDTO,
  CreateProjectDTO,
  UpdateProjectDTO,
  CreateCombDTO,
  UpdateCombDTO,
  CreatePaneDTO,
  UpdatePaneDTO,

  PaginationOptions,
  ProjectsQueryOptions,
  CombsQueryOptions,
  PanesQueryOptions,
} from "./types";

import {
  initDatabase,
  closeDatabase,
  getDatabasePath,
  isDatabaseConnected,
} from "./connection";
import { ProvidersRepository } from "./repositories/providers";
import { ProjectsRepository } from "./repositories/projects";
import { CombsRepository } from "./repositories/combs";
import { PanesRepository } from "./repositories/panes";

/**
 * Objeto DB para acesso simplificado aos repositórios.
 */
export const db = {
  providers: ProvidersRepository,
  projects: ProjectsRepository,
  combs: CombsRepository,
  panes: PanesRepository,

  init: initDatabase,
  close: closeDatabase,

  getPath: (): string | null => {
    if (!isDatabaseConnected()) {
      return null;
    }
    return getDatabasePath();
  },
};

export default db;
