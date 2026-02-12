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

  // Migração: atualizar CHECK de providers se o banco foi criado com schema antigo (sem codex/cursor)
  migrateProvidersTypeCheck(db);

  // Migração: adicionar coluna api_key_encrypted se não existir
  migrateApiKeyEncrypted(db);

  // Migração: adicionar coluna preserve_instructions em missions se não existir
  migratePreserveInstructions(db);

  // Migração: adicionar coluna code_generation_attempts em missions se não existir
  migrateCodeGenerationAttempts(db);

  // Migração: adicionar colunas is_committed e is_pushed em missions se não existirem
  migrateCommitPushFlags(db);

  // Migração: adicionar plan_provider_id e code_provider_id em missions se não existirem
  migratePlanCodeProviderIds(db);

  // Migração: criar tabela activation se não existir
  migrateActivationTable(db);

  console.log(`[Database] Initialized at: ${dbPath}`);

  return db;
}

/**
 * Migração única: recria a tabela providers com CHECK que inclui 'codex' e 'cursor'.
 * Necessário porque SQLite não permite ALTER TABLE para mudar CHECK em tabelas existentes.
 */
function migrateProvidersTypeCheck(database: Database.Database): void {
  const row = database
    .prepare(
      "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'providers'",
    )
    .get() as { sql: string } | undefined;
  if (!row?.sql || row.sql.includes("'cursor'")) {
    return; // já tem o novo CHECK ou tabela não existe
  }

  database.pragma('foreign_keys = OFF');
  database.exec(`
    CREATE TABLE providers_new (
      id TEXT PRIMARY KEY,
      name TEXT NOT NULL,
      type TEXT NOT NULL CHECK (type IN ('claude-code', 'codex', 'openai', 'anthropic', 'cursor', 'custom')),
      api_key TEXT,
      cli_path TEXT,
      config TEXT,
      is_active INTEGER DEFAULT 1,
      created_at TEXT DEFAULT (datetime('now')),
      updated_at TEXT DEFAULT (datetime('now'))
    );
    INSERT INTO providers_new SELECT * FROM providers;
    DROP TABLE providers;
    ALTER TABLE providers_new RENAME TO providers;
    CREATE TRIGGER update_providers_timestamp
    AFTER UPDATE ON providers
    BEGIN
      UPDATE providers SET updated_at = datetime('now') WHERE id = NEW.id;
    END;
  `);
  database.pragma('foreign_keys = ON');
  console.log('[Database] Migration: providers type CHECK updated (codex, cursor added).');
}

/**
 * Migração: adiciona coluna api_key_encrypted se não existir.
 */
function migrateApiKeyEncrypted(database: Database.Database): void {
  const row = database
    .prepare("PRAGMA table_info(providers)")
    .all() as Array<{ name: string }>;
  const hasColumn = row.some((c) => c.name === "api_key_encrypted");
  if (hasColumn) return;

  database.exec("ALTER TABLE providers ADD COLUMN api_key_encrypted BLOB");
  console.log("[Database] Migration: api_key_encrypted column added.");
}

/**
 * Migração: adiciona coluna preserve_instructions em missions se não existir.
 */
function migratePreserveInstructions(database: Database.Database): void {
  const rows = database
    .prepare("PRAGMA table_info(missions)")
    .all() as Array<{ name: string }>;
  const hasColumn = rows.some((c) => c.name === "preserve_instructions");
  if (hasColumn) return;

  database.exec("ALTER TABLE missions ADD COLUMN preserve_instructions TEXT");
  console.log("[Database] Migration: preserve_instructions column added to missions.");
}

/**
 * Migração: adiciona coluna code_generation_attempts em missions se não existir.
 */
function migrateCodeGenerationAttempts(database: Database.Database): void {
  const rows = database
    .prepare("PRAGMA table_info(missions)")
    .all() as Array<{ name: string }>;
  const hasColumn = rows.some((c) => c.name === "code_generation_attempts");
  if (hasColumn) return;

  database.exec("ALTER TABLE missions ADD COLUMN code_generation_attempts INTEGER DEFAULT 0");
  console.log("[Database] Migration: code_generation_attempts column added to missions.");
}

/**
 * Migração: adiciona colunas is_committed e is_pushed em missions se não existirem.
 */
function migrateCommitPushFlags(database: Database.Database): void {
  const rows = database
    .prepare("PRAGMA table_info(missions)")
    .all() as Array<{ name: string }>;
  
  const hasIsCommitted = rows.some((c) => c.name === "is_committed");
  const hasIsPushed = rows.some((c) => c.name === "is_pushed");
  
  if (!hasIsCommitted) {
    database.exec("ALTER TABLE missions ADD COLUMN is_committed INTEGER DEFAULT 0");
    console.log("[Database] Migration: is_committed column added to missions.");
  }
  
  if (!hasIsPushed) {
    database.exec("ALTER TABLE missions ADD COLUMN is_pushed INTEGER DEFAULT 0");
    console.log("[Database] Migration: is_pushed column added to missions.");
  }
}

/**
 * Migração: adiciona plan_provider_id e code_provider_id em missions se não existirem.
 */
function migratePlanCodeProviderIds(database: Database.Database): void {
  const rows = database
    .prepare("PRAGMA table_info(missions)")
    .all() as Array<{ name: string }>;

  const hasPlanProviderId = rows.some((c) => c.name === "plan_provider_id");
  const hasCodeProviderId = rows.some((c) => c.name === "code_provider_id");

  if (!hasPlanProviderId) {
    database.exec("ALTER TABLE missions ADD COLUMN plan_provider_id TEXT");
    console.log("[Database] Migration: plan_provider_id column added to missions.");
  }

  if (!hasCodeProviderId) {
    database.exec("ALTER TABLE missions ADD COLUMN code_provider_id TEXT");
    console.log("[Database] Migration: code_provider_id column added to missions.");
  }
}

/**
 * Migração: cria tabela activation (beta/licença) se não existir.
 */
function migrateActivationTable(database: Database.Database): void {
  const row = database
    .prepare(
      "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'activation'",
    )
    .get();
  if (row) return;

  database.exec(`
    CREATE TABLE activation (
      id INTEGER PRIMARY KEY CHECK (id = 1),
      email TEXT NOT NULL,
      machine_id TEXT NOT NULL,
      activated INTEGER DEFAULT 1,
      token TEXT,
      activated_at TEXT DEFAULT (datetime('now')),
      created_at TEXT DEFAULT (datetime('now')),
      updated_at TEXT DEFAULT (datetime('now'))
    );
  `);
  console.log("[Database] Migration: activation table created.");
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
