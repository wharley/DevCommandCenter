"use strict";
// Dev Command Center - Database Module
// Ponto de entrada para toda a camada de persistência SQLite
Object.defineProperty(exports, "__esModule", { value: true });
exports.db = exports.MissionLogsRepository = exports.MissionsRepository = exports.ProjectsRepository = exports.ProvidersRepository = exports.generateId = exports.getDatabasePath = exports.getAppDataPath = exports.withTransaction = exports.isDatabaseConnected = exports.closeDatabase = exports.getDatabase = exports.initDatabase = void 0;
// Conexão e utilitários
var connection_1 = require("./connection");
Object.defineProperty(exports, "initDatabase", { enumerable: true, get: function () { return connection_1.initDatabase; } });
Object.defineProperty(exports, "getDatabase", { enumerable: true, get: function () { return connection_1.getDatabase; } });
Object.defineProperty(exports, "closeDatabase", { enumerable: true, get: function () { return connection_1.closeDatabase; } });
Object.defineProperty(exports, "isDatabaseConnected", { enumerable: true, get: function () { return connection_1.isDatabaseConnected; } });
Object.defineProperty(exports, "withTransaction", { enumerable: true, get: function () { return connection_1.withTransaction; } });
Object.defineProperty(exports, "getAppDataPath", { enumerable: true, get: function () { return connection_1.getAppDataPath; } });
Object.defineProperty(exports, "getDatabasePath", { enumerable: true, get: function () { return connection_1.getDatabasePath; } });
Object.defineProperty(exports, "generateId", { enumerable: true, get: function () { return connection_1.generateId; } });
// Repositórios
var providers_1 = require("./repositories/providers");
Object.defineProperty(exports, "ProvidersRepository", { enumerable: true, get: function () { return providers_1.ProvidersRepository; } });
var projects_1 = require("./repositories/projects");
Object.defineProperty(exports, "ProjectsRepository", { enumerable: true, get: function () { return projects_1.ProjectsRepository; } });
var missions_1 = require("./repositories/missions");
Object.defineProperty(exports, "MissionsRepository", { enumerable: true, get: function () { return missions_1.MissionsRepository; } });
var mission_logs_1 = require("./repositories/mission-logs");
Object.defineProperty(exports, "MissionLogsRepository", { enumerable: true, get: function () { return mission_logs_1.MissionLogsRepository; } });
// ============================================
// Funções de conveniência para uso no Electron
// ============================================
const connection_2 = require("./connection");
const providers_2 = require("./repositories/providers");
const projects_2 = require("./repositories/projects");
const missions_2 = require("./repositories/missions");
const mission_logs_2 = require("./repositories/mission-logs");
/**
 * Objeto DB para acesso simplificado aos repositórios.
 * Uso: db.providers.findAll(), db.projects.create(...), etc.
 */
exports.db = {
    providers: providers_2.ProvidersRepository,
    projects: projects_2.ProjectsRepository,
    missions: missions_2.MissionsRepository,
    missionLogs: mission_logs_2.MissionLogsRepository,
    /**
     * Inicializa o banco de dados.
     * Deve ser chamado uma vez na inicialização do app Electron.
     */
    init: connection_2.initDatabase,
    /**
     * Fecha a conexão com o banco.
     * Deve ser chamado ao fechar o app Electron.
     */
    close: connection_2.closeDatabase,
};
// Default export para uso mais limpo
exports.default = exports.db;
//# sourceMappingURL=index.js.map