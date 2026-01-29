// Dev Command Center - Database Connection
// Gerencia a conexão com o SQLite usando better-sqlite3

import Database from 'better-sqlite3';
import path from 'path';
import fs from 'fs';

// Singleton para a conexão do banco
let db: Database.Database | null = null;

// Cache do userData path (definido pelo Electron main process)
let userDataPath: string | null = null;

/**
 * Define o caminho do userData (chamado pelo main process do Electron).
 */
export function setUserDataPath(appPath: string): void {
  userDataPath = appPath;
  console.log('[Database] User data path set to:', appPath);
}

/**
 * Obtém o diretório de dados do app de acordo com o sistema operacional.
 * - Windows: %APPDATA%/dev-command-center
 * - macOS: ~/Library/Application Support/dev-command-center
 * - Linux: ~/.local/share/dev-command-center
 */
export function getAppDataPath(): string {
  const appName = 'dev-command-center';
  
  // Se o Electron definiu o path, usar ele
  if (userDataPath) {
    return userDataPath;
  }
  
  // Tentar usar app.getPath do Electron diretamente (main process)
  try {
    // eslint-disable-next-line @typescript-eslint/no-var-requires
    const { app } = require('electron');
    if (app && typeof app.getPath === 'function') {
      const electronPath = app.getPath('userData');
      userDataPath = electronPath;
      return electronPath;
    }
  } catch {
    // Electron não disponível, usar fallback
  }
  
  // Fallback para Node.js puro (desenvolvimento/testes)
  const platform = process.platform;
  const homeDir = process.env.HOME || process.env.USERPROFILE || '';
  
  switch (platform) {
    case 'win32':
      return path.join(process.env.APPDATA || path.join(homeDir, 'AppData', 'Roaming'), appName);
    case 'darwin':
      return path.join(homeDir, 'Library', 'Application Support', appName);
    default: // linux e outros
      return path.join(process.env.XDG_DATA_HOME || path.join(homeDir, '.local', 'share'), appName);
  }
}

/**
 * Obtém o caminho completo do arquivo do banco de dados.
 */
export function getDatabasePath(): string {
  const appDataPath = getAppDataPath();
  return path.join(appDataPath, 'data.db');
}

/**
 * Lê o schema SQL do arquivo.
 */
function getSchema(): string {
  // Em produção, o schema estará junto com o app
  const schemaPath = path.join(__dirname, 'schema.sql');
  
  // Fallback para desenvolvimento
  if (!fs.existsSync(schemaPath)) {
    const devSchemaPath = path.join(process.cwd(), 'lib', 'database', 'schema.sql');
    if (fs.existsSync(devSchemaPath)) {
      return fs.readFileSync(devSchemaPath, 'utf-8');
    }
    throw new Error('Schema file not found');
  }
  
  return fs.readFileSync(schemaPath, 'utf-8');
}

/**
 * Inicializa a conexão com o banco de dados.
 * Cria o diretório e o banco se não existirem.
 */
export function initDatabase(): Database.Database {
  if (db) {
    return db;
  }
  
  const dbPath = getDatabasePath();
  const dbDir = path.dirname(dbPath);
  
  // Cria o diretório se não existir
  if (!fs.existsSync(dbDir)) {
    fs.mkdirSync(dbDir, { recursive: true });
  }
  
  // Abre/cria o banco de dados
  db = new Database(dbPath);
  
  // Configurações de performance e segurança
  db.pragma('journal_mode = WAL'); // Write-Ahead Logging para melhor performance
  db.pragma('foreign_keys = ON');  // Habilita foreign keys
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
export function getDatabase(): Database.Database {
  if (!db) {
    return initDatabase();
  }
  return db;
}

/**
 * Fecha a conexão com o banco de dados.
 */
export function closeDatabase(): void {
  if (db) {
    db.close();
    db = null;
    console.log('[Database] Connection closed');
  }
}

/**
 * Verifica se o banco de dados está conectado.
 */
export function isDatabaseConnected(): boolean {
  return db !== null && db.open;
}

/**
 * Executa uma função dentro de uma transação.
 */
export function withTransaction<T>(fn: () => T): T {
  const database = getDatabase();
  return database.transaction(fn)();
}

/**
 * Utilitário para gerar IDs únicos (UUID v4 simples).
 */
export function generateId(): string {
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
    const r = Math.random() * 16 | 0;
    const v = c === 'x' ? r : (r & 0x3 | 0x8);
    return v.toString(16);
  });
}
