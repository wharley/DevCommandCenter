-- Dev Command Center - SQLite Schema
-- Este arquivo define a estrutura do banco de dados local

-- Tabela de Provedores de IA (Claude Code, OpenAI, etc.)
CREATE TABLE IF NOT EXISTS providers (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  type TEXT NOT NULL CHECK (type IN ('claude-code', 'codex', 'openai', 'anthropic', 'custom')),
  api_key TEXT,
  cli_path TEXT,
  config TEXT, -- JSON com configurações adicionais
  is_active INTEGER DEFAULT 1,
  created_at TEXT DEFAULT (datetime('now')),
  updated_at TEXT DEFAULT (datetime('now'))
);

-- Tabela de Projetos (repositórios locais)
CREATE TABLE IF NOT EXISTS projects (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  path TEXT NOT NULL UNIQUE,
  description TEXT,
  default_provider_id TEXT,
  git_remote_url TEXT,
  last_opened_at TEXT,
  created_at TEXT DEFAULT (datetime('now')),
  updated_at TEXT DEFAULT (datetime('now')),
  FOREIGN KEY (default_provider_id) REFERENCES providers(id) ON DELETE SET NULL
);

-- Tabela de Missões de Código
CREATE TABLE IF NOT EXISTS missions (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  provider_id TEXT,
  title TEXT NOT NULL,
  description TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'created' CHECK (status IN ('created', 'planning', 'plan_generated', 'generating_code', 'code_ready', 'applying', 'completed', 'failed', 'cancelled')),
  plan TEXT, -- JSON com o plano de ação gerado
  generated_code TEXT, -- JSON com as sugestões de código/diffs
  context TEXT, -- JSON com contexto do repo usado na missão
  error_message TEXT,
  started_at TEXT,
  completed_at TEXT,
  created_at TEXT DEFAULT (datetime('now')),
  updated_at TEXT DEFAULT (datetime('now')),
  FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
  FOREIGN KEY (provider_id) REFERENCES providers(id) ON DELETE SET NULL
);

-- Tabela de Logs das Missões (histórico de interações)
CREATE TABLE IF NOT EXISTS mission_logs (
  id TEXT PRIMARY KEY,
  mission_id TEXT NOT NULL,
  type TEXT NOT NULL CHECK (type IN ('info', 'prompt', 'response', 'error', 'action', 'user_input')),
  content TEXT NOT NULL,
  metadata TEXT, -- JSON com dados extras (tokens usados, tempo, etc.)
  created_at TEXT DEFAULT (datetime('now')),
  FOREIGN KEY (mission_id) REFERENCES missions(id) ON DELETE CASCADE
);

-- Índices para performance
CREATE INDEX IF NOT EXISTS idx_projects_last_opened ON projects(last_opened_at DESC);
CREATE INDEX IF NOT EXISTS idx_missions_project ON missions(project_id);
CREATE INDEX IF NOT EXISTS idx_missions_status ON missions(status);
CREATE INDEX IF NOT EXISTS idx_missions_created ON missions(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_mission_logs_mission ON mission_logs(mission_id);
CREATE INDEX IF NOT EXISTS idx_mission_logs_created ON mission_logs(created_at DESC);

-- Trigger para atualizar updated_at automaticamente
CREATE TRIGGER IF NOT EXISTS update_providers_timestamp
AFTER UPDATE ON providers
BEGIN
  UPDATE providers SET updated_at = datetime('now') WHERE id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS update_projects_timestamp
AFTER UPDATE ON projects
BEGIN
  UPDATE projects SET updated_at = datetime('now') WHERE id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS update_missions_timestamp
AFTER UPDATE ON missions
BEGIN
  UPDATE missions SET updated_at = datetime('now') WHERE id = NEW.id;
END;
