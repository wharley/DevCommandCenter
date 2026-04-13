// Dev Command Center - Database Connection
// Gerencia a conexão com o SQLite usando better-sqlite3

import Database from 'better-sqlite3';
import path from 'path';
import fs from 'fs';

// Singleton para a conexão do banco
let db: Database.Database | null = null;

// Cache do userData path (definido pelo host em integrações legadas)
let userDataPath: string | null = null;

/**
 * Define o caminho do userData (ex.: host Node / scripts).
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
  
  // Se o host definiu o path, usar ele
  if (userDataPath) {
    return userDataPath;
  }
  
  // Fallback para Node.js (scripts / testes; o app Tauri usa Rust + window.db)
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

  // Migração: atualizar CHECK de providers para incluir 'gemini'
  migrateProvidersTypeCheckAddGemini(db);

  // Migração: adicionar coluna preserve_instructions em missions se não existir
  migratePreserveInstructions(db);

  // Migração: adicionar coluna code_generation_attempts em missions se não existir
  migrateCodeGenerationAttempts(db);

  // Migração: adicionar colunas is_committed e is_pushed em missions se não existirem
  migrateCommitPushFlags(db);

  // Migração: adicionar plan_provider_id e code_provider_id em missions se não existirem
  migratePlanCodeProviderIds(db);

  // Migração: adicionar mission_type em missions se não existir
  migrateMissionType(db);

  // Migração: adicionar worktree_path e worktree_branch em missions se não existirem (antes de agents_cli para a recriação ter 24 colunas)
  migrateMissionWorktree(db);

  // Migração: atualizar CHECK de missions para incluir 'agents_cli'
  migrateMissionTypeAgentsCli(db);

  // Migração: criar tabela activation se não existir
  migrateActivationTable(db);

  // Migração: colunas do Agents Wall (base_branch, target_branch, last_output_summary, last_git_summary, wall_status)
  migrateMissionWallColumns(db);

  // Migração: criar tabelas combs e panes (Hive/Comb/Pane architecture)
  migrateCombsPanes(db);

  // Migração: review_targets (JSON) em combs para multi-repo na mesma Review
  migrateCombsReviewTargets(db);

  // Migração: is_pinned e pinned_at em combs para fixar workspaces
  migrateCombsPinned(db);

  // Migração: última atividade Git por worktree (sidebar / ordenação)
  migrateCombsLastGitActivity(db);

  // Migração: ligação automática PR/MR (JSON em combs.forge_link)
  migrateCombsForgeLink(db);

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
 * Migração: recria a tabela providers com CHECK que inclui 'gemini'.
 * Necessário porque SQLite não permite ALTER TABLE para mudar CHECK em tabelas existentes.
 */
function migrateProvidersTypeCheckAddGemini(database: Database.Database): void {
  const row = database
    .prepare(
      "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'providers'",
    )
    .get() as { sql: string } | undefined;
  if (!row?.sql || row.sql.includes("'gemini'")) {
    return; // já tem gemini no CHECK ou tabela não existe
  }

  database.pragma('foreign_keys = OFF');
  database.exec(`
    CREATE TABLE providers_new (
      id TEXT PRIMARY KEY,
      name TEXT NOT NULL,
      type TEXT NOT NULL CHECK (type IN ('claude-code', 'codex', 'openai', 'anthropic', 'cursor', 'gemini', 'custom')),
      api_key TEXT,
      api_key_encrypted BLOB,
      cli_path TEXT,
      config TEXT,
      is_active INTEGER DEFAULT 1,
      created_at TEXT DEFAULT (datetime('now')),
      updated_at TEXT DEFAULT (datetime('now'))
    );
    INSERT INTO providers_new SELECT id, name, type, api_key, api_key_encrypted, cli_path, config, is_active, created_at, updated_at FROM providers;
    DROP TABLE providers;
    ALTER TABLE providers_new RENAME TO providers;
    CREATE TRIGGER update_providers_timestamp
    AFTER UPDATE ON providers
    BEGIN
      UPDATE providers SET updated_at = datetime('now') WHERE id = NEW.id;
    END;
  `);
  database.pragma('foreign_keys = ON');
  console.log('[Database] Migration: providers type CHECK updated (gemini added).');
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
 * Migração: adiciona mission_type em missions se não existir.
 */
function migrateMissionType(database: Database.Database): void {
  const rows = database
    .prepare("PRAGMA table_info(missions)")
    .all() as Array<{ name: string }>;
  const hasColumn = rows.some((c) => c.name === "mission_type");
  if (hasColumn) return;

  database.exec(
    "ALTER TABLE missions ADD COLUMN mission_type TEXT DEFAULT 'implementation'",
  );
  console.log("[Database] Migration: mission_type column added to missions.");
}

/**
 * Migração: recria a tabela missions com CHECK que inclui 'agents_cli' em mission_type.
 * Usa as colunas reais da tabela atual para o INSERT, assim funciona com 23 ou 24 colunas.
 */
function migrateMissionTypeAgentsCli(database: Database.Database): void {
  const row = database
    .prepare(
      "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'missions'",
    )
    .get() as { sql: string } | undefined;
  if (!row?.sql || row.sql.includes("'agents_cli'")) {
    return;
  }

  const columns = database
    .prepare("PRAGMA table_info(missions)")
    .all() as Array<{ name: string }>;
  const colList = columns.map((c) => c.name).join(", ");

  database.pragma('foreign_keys = OFF');
  database.exec(`
    DROP TABLE IF EXISTS missions_new;
    CREATE TABLE missions_new (
      id TEXT PRIMARY KEY,
      project_id TEXT NOT NULL,
      provider_id TEXT,
      plan_provider_id TEXT,
      code_provider_id TEXT,
      title TEXT NOT NULL,
      description TEXT NOT NULL,
      status TEXT NOT NULL DEFAULT 'created' CHECK (status IN ('created', 'planning', 'plan_generated', 'generating_code', 'code_ready', 'applying', 'completed', 'failed', 'cancelled')),
      mission_type TEXT DEFAULT 'implementation' CHECK (mission_type IN ('implementation', 'analysis', 'agents_cli')),
      plan TEXT,
      generated_code TEXT,
      context TEXT,
      preserve_instructions TEXT,
      error_message TEXT,
      code_generation_attempts INTEGER DEFAULT 0,
      is_committed INTEGER DEFAULT 0,
      is_pushed INTEGER DEFAULT 0,
      pending_commands TEXT,
      worktree_path TEXT,
      worktree_branch TEXT,
      started_at TEXT,
      completed_at TEXT,
      created_at TEXT DEFAULT (datetime('now')),
      updated_at TEXT DEFAULT (datetime('now')),
      FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
      FOREIGN KEY (provider_id) REFERENCES providers(id) ON DELETE SET NULL,
      FOREIGN KEY (plan_provider_id) REFERENCES providers(id) ON DELETE SET NULL,
      FOREIGN KEY (code_provider_id) REFERENCES providers(id) ON DELETE SET NULL
    );
  `);
  database.exec(
    `INSERT INTO missions_new (${colList}) SELECT ${colList} FROM missions`,
  );
  database.exec(`
    DROP TABLE missions;
    ALTER TABLE missions_new RENAME TO missions;
  `);
  database.pragma('foreign_keys = ON');
  console.log("[Database] Migration: missions mission_type CHECK updated (agents_cli added).");
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
 * Migração: adiciona worktree_path e worktree_branch em missions (worktrees por missão).
 */
function migrateMissionWorktree(database: Database.Database): void {
  const rows = database
    .prepare("PRAGMA table_info(missions)")
    .all() as Array<{ name: string }>;
  const hasWorktreePath = rows.some((c) => c.name === "worktree_path");
  const hasWorktreeBranch = rows.some((c) => c.name === "worktree_branch");
  if (!hasWorktreePath) {
    database.exec("ALTER TABLE missions ADD COLUMN worktree_path TEXT");
    console.log("[Database] Migration: worktree_path column added to missions.");
  }
  if (!hasWorktreeBranch) {
    database.exec("ALTER TABLE missions ADD COLUMN worktree_branch TEXT");
    console.log("[Database] Migration: worktree_branch column added to missions.");
  }
}

/**
 * Migração: colunas do Agents Wall (base_branch, target_branch, last_output_summary, last_git_summary, wall_status).
 */
function migrateMissionWallColumns(database: Database.Database): void {
  const rows = database
    .prepare("PRAGMA table_info(missions)")
    .all() as Array<{ name: string }>;
  const names = new Set(rows.map((c) => c.name));
  const columns = [
    { name: "base_branch", sql: "ALTER TABLE missions ADD COLUMN base_branch TEXT" },
    { name: "target_branch", sql: "ALTER TABLE missions ADD COLUMN target_branch TEXT" },
    { name: "last_output_summary", sql: "ALTER TABLE missions ADD COLUMN last_output_summary TEXT" },
    { name: "last_git_summary", sql: "ALTER TABLE missions ADD COLUMN last_git_summary TEXT" },
    { name: "wall_status", sql: "ALTER TABLE missions ADD COLUMN wall_status TEXT" },
  ];
  for (const col of columns) {
    if (!names.has(col.name)) {
      database.exec(col.sql);
      console.log(`[Database] Migration: ${col.name} column added to missions.`);
    }
  }
}

/**
 * Migração: cria tabelas combs e panes se não existirem (Hive/Comb/Pane architecture).
 */
function migrateCombsPanes(database: Database.Database): void {
  const combsExists = database
    .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'combs'")
    .get();
  if (!combsExists) {
    database.exec(`
      CREATE TABLE combs (
        id TEXT PRIMARY KEY,
        project_id TEXT NOT NULL,
        name TEXT NOT NULL,
        description TEXT,
        base_branch TEXT NOT NULL DEFAULT 'main',
        branch TEXT,
        worktree_path TEXT,
        status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'ready_for_review', 'applied', 'discarded', 'archived', 'error')),
        last_opened_at TEXT,
        created_at TEXT DEFAULT (datetime('now')),
        updated_at TEXT DEFAULT (datetime('now')),
        FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
      );
      CREATE INDEX idx_combs_project ON combs(project_id);
      CREATE INDEX idx_combs_status ON combs(status);
      CREATE INDEX idx_combs_last_opened ON combs(last_opened_at DESC);
      CREATE TRIGGER update_combs_timestamp
      AFTER UPDATE ON combs
      BEGIN
        UPDATE combs SET updated_at = datetime('now') WHERE id = NEW.id;
      END;
    `);
    console.log("[Database] Migration: combs table created.");
  }

  const panesExists = database
    .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'panes'")
    .get();
  if (!panesExists) {
    database.exec(`
      CREATE TABLE panes (
        id TEXT PRIMARY KEY,
        comb_id TEXT NOT NULL,
        type TEXT NOT NULL DEFAULT 'term' CHECK (type IN ('term', 'agent')),
        provider_id TEXT,
        title TEXT,
        initial_prompt TEXT,
        cwd TEXT,
        pty_owner_key TEXT,
        status TEXT NOT NULL DEFAULT 'idle' CHECK (status IN ('idle', 'running', 'exited')),
        layout_order INTEGER NOT NULL DEFAULT 0,
        last_activity_at TEXT,
        created_at TEXT DEFAULT (datetime('now')),
        updated_at TEXT DEFAULT (datetime('now')),
        FOREIGN KEY (comb_id) REFERENCES combs(id) ON DELETE CASCADE,
        FOREIGN KEY (provider_id) REFERENCES providers(id) ON DELETE SET NULL
      );
      CREATE INDEX idx_panes_comb ON panes(comb_id);
      CREATE INDEX idx_panes_layout ON panes(comb_id, layout_order);
      CREATE TRIGGER update_panes_timestamp
      AFTER UPDATE ON panes
      BEGIN
        UPDATE panes SET updated_at = datetime('now') WHERE id = NEW.id;
      END;
    `);
    console.log("[Database] Migration: panes table created.");
  }
}

/**
 * Adiciona coluna review_targets (JSON) em combs para targets genéricos de revisão.
 */
function migrateCombsReviewTargets(database: Database.Database): void {
  const combsExists = database
    .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'combs'")
    .get();
  if (!combsExists) return;

  const columns = database.prepare("PRAGMA table_info(combs)").all() as { name: string }[];
  const names = new Set(columns.map((c) => c.name));
  if (!names.has("review_targets")) {
    database.exec(`ALTER TABLE combs ADD COLUMN review_targets TEXT`);
    console.log("[Database] Migration: review_targets column added to combs.");
  }
}

/**
 * Adiciona colunas is_pinned e pinned_at em combs para permitir fixar workspaces.
 */
function migrateCombsPinned(database: Database.Database): void {
  const combsExists = database
    .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'combs'")
    .get();
  if (!combsExists) return;

  const columns = database.prepare("PRAGMA table_info(combs)").all() as { name: string }[];
  const names = new Set(columns.map((c) => c.name));

  if (!names.has("is_pinned")) {
    database.exec(`ALTER TABLE combs ADD COLUMN is_pinned INTEGER DEFAULT 0`);
    console.log("[Database] Migration: is_pinned column added to combs.");
  }

  if (!names.has("pinned_at")) {
    database.exec(`ALTER TABLE combs ADD COLUMN pinned_at TEXT`);
    console.log("[Database] Migration: pinned_at column added to combs.");
  }

  // Criar índice para performance ao ordenar por pinned
  const indexExists = database
    .prepare("SELECT name FROM sqlite_master WHERE type = 'index' AND name = 'idx_combs_pinned'")
    .get();
  if (!indexExists) {
    database.exec(`CREATE INDEX idx_combs_pinned ON combs(is_pinned DESC, pinned_at DESC)`);
    console.log("[Database] Migration: idx_combs_pinned index created.");
  }
}

/**
 * Coluna last_git_activity_at em combs (timestamp da última atividade Git no worktree).
 */
function migrateCombsLastGitActivity(database: Database.Database): void {
  const combsExists = database
    .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'combs'")
    .get();
  if (!combsExists) return;

  const columns = database.prepare("PRAGMA table_info(combs)").all() as { name: string }[];
  const names = new Set(columns.map((c) => c.name));

  if (!names.has("last_git_activity_at")) {
    database.exec(`ALTER TABLE combs ADD COLUMN last_git_activity_at TEXT`);
    console.log("[Database] Migration: last_git_activity_at column added to combs.");
  }

  const indexExists = database
    .prepare("SELECT name FROM sqlite_master WHERE type = 'index' AND name = 'idx_combs_last_git_activity'")
    .get();
  if (!indexExists) {
    database.exec(`CREATE INDEX idx_combs_last_git_activity ON combs(last_git_activity_at DESC)`);
    console.log("[Database] Migration: idx_combs_last_git_activity index created.");
  }
}

/** JSON com PR/MR aberto ligado à branch do worktree (GitHub/GitLab). */
function migrateCombsForgeLink(database: Database.Database): void {
  const combsExists = database
    .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'combs'")
    .get();
  if (!combsExists) return;

  const columns = database.prepare("PRAGMA table_info(combs)").all() as { name: string }[];
  const names = new Set(columns.map((c) => c.name));

  if (!names.has("forge_link")) {
    database.exec(`ALTER TABLE combs ADD COLUMN forge_link TEXT`);
    console.log("[Database] Migration: forge_link column added to combs.");
  }
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
