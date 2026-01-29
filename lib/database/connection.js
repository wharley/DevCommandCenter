"use strict";
// Dev Command Center - Database Connection
// Gerencia a conexão com o SQLite usando better-sqlite3
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.getAppDataPath = getAppDataPath;
exports.getDatabasePath = getDatabasePath;
exports.initDatabase = initDatabase;
exports.getDatabase = getDatabase;
exports.closeDatabase = closeDatabase;
exports.isDatabaseConnected = isDatabaseConnected;
exports.withTransaction = withTransaction;
exports.generateId = generateId;
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
const path_1 = __importDefault(require("path"));
const fs_1 = __importDefault(require("fs"));
// Singleton para a conexão do banco
let db = null;
/**
 * Obtém o diretório de dados do app de acordo com o sistema operacional.
 * - Windows: %APPDATA%/dev-command-center
 * - macOS: ~/Library/Application Support/dev-command-center
 * - Linux: ~/.local/share/dev-command-center
 */
function getAppDataPath() {
    const appName = 'dev-command-center';
    // Em ambiente Electron, usar app.getPath('userData')
    // @ts-expect-error - electron pode não estar disponível
    if (typeof window !== 'undefined' && window.electron?.getAppDataPath) {
        // @ts-expect-error - electron pode não estar disponível
        return window.electron.getAppDataPath();
    }
    // Fallback para Node.js puro (desenvolvimento/testes)
    const platform = process.platform;
    const homeDir = process.env.HOME || process.env.USERPROFILE || '';
    switch (platform) {
        case 'win32':
            return path_1.default.join(process.env.APPDATA || path_1.default.join(homeDir, 'AppData', 'Roaming'), appName);
        case 'darwin':
            return path_1.default.join(homeDir, 'Library', 'Application Support', appName);
        default: // linux e outros
            return path_1.default.join(process.env.XDG_DATA_HOME || path_1.default.join(homeDir, '.local', 'share'), appName);
    }
}
/**
 * Obtém o caminho completo do arquivo do banco de dados.
 */
function getDatabasePath() {
    const appDataPath = getAppDataPath();
    return path_1.default.join(appDataPath, 'data.db');
}
/**
 * Lê o schema SQL do arquivo.
 */
function getSchema() {
    // Em produção, o schema estará junto com o app
    const schemaPath = path_1.default.join(__dirname, 'schema.sql');
    // Fallback para desenvolvimento
    if (!fs_1.default.existsSync(schemaPath)) {
        const devSchemaPath = path_1.default.join(process.cwd(), 'lib', 'database', 'schema.sql');
        if (fs_1.default.existsSync(devSchemaPath)) {
            return fs_1.default.readFileSync(devSchemaPath, 'utf-8');
        }
        throw new Error('Schema file not found');
    }
    return fs_1.default.readFileSync(schemaPath, 'utf-8');
}
/**
 * Inicializa a conexão com o banco de dados.
 * Cria o diretório e o banco se não existirem.
 */
function initDatabase() {
    if (db) {
        return db;
    }
    const dbPath = getDatabasePath();
    const dbDir = path_1.default.dirname(dbPath);
    // Cria o diretório se não existir
    if (!fs_1.default.existsSync(dbDir)) {
        fs_1.default.mkdirSync(dbDir, { recursive: true });
    }
    // Abre/cria o banco de dados
    db = new better_sqlite3_1.default(dbPath);
    // Configurações de performance e segurança
    db.pragma('journal_mode = WAL'); // Write-Ahead Logging para melhor performance
    db.pragma('foreign_keys = ON'); // Habilita foreign keys
    db.pragma('synchronous = NORMAL'); // Balanço entre segurança e performance
    // Executa o schema para criar/atualizar tabelas
    const schema = getSchema();
    db.exec(schema);
    console.log(`[Database] Initialized at: ${dbPath}`);
    return db;
}
/**
 * Obtém a instância do banco de dados.
 * Inicializa se ainda não foi inicializado.
 */
function getDatabase() {
    if (!db) {
        return initDatabase();
    }
    return db;
}
/**
 * Fecha a conexão com o banco de dados.
 */
function closeDatabase() {
    if (db) {
        db.close();
        db = null;
        console.log('[Database] Connection closed');
    }
}
/**
 * Verifica se o banco de dados está conectado.
 */
function isDatabaseConnected() {
    return db !== null && db.open;
}
/**
 * Executa uma função dentro de uma transação.
 */
function withTransaction(fn) {
    const database = getDatabase();
    return database.transaction(fn)();
}
/**
 * Utilitário para gerar IDs únicos (UUID v4 simples).
 */
function generateId() {
    return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
        const r = Math.random() * 16 | 0;
        const v = c === 'x' ? r : (r & 0x3 | 0x8);
        return v.toString(16);
    });
}
//# sourceMappingURL=connection.js.map