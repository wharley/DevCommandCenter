-- Dev Command Center - SQLite Schema
-- Este arquivo define a estrutura do banco de dados local

-- Tabela de Provedores de IA (Claude Code, OpenAI, etc.)
-- api_key: legado (texto plano, migração); api_key_encrypted: criptografado (preferido)
CREATE TABLE IF NOT EXISTS providers (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  type TEXT NOT NULL CHECK (type IN ('claude-code', 'codex', 'openai', 'anthropic', 'cursor', 'gemini', 'custom')),
  api_key TEXT,
  api_key_encrypted BLOB,
  cli_path TEXT,
  config TEXT, -- JSON com configurações adicionais
  is_active INTEGER DEFAULT 1,
  created_at TEXT DEFAULT (datetime('now')),
  updated_at TEXT DEFAULT (datetime('now'))
);

-- Tabela de Projetos (repositórios locais; repo_config é espelho da config canônica .dcc.toml)
CREATE TABLE IF NOT EXISTS projects (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  path TEXT NOT NULL UNIQUE,
  description TEXT,
  default_provider_id TEXT,
  git_remote_url TEXT,
  repo_config TEXT,
  last_opened_at TEXT,
  created_at TEXT DEFAULT (datetime('now')),
  updated_at TEXT DEFAULT (datetime('now')),
  FOREIGN KEY (default_provider_id) REFERENCES providers(id) ON DELETE SET NULL
);

-- Tabela de Ativação (beta/licença) — singleton por instalação
CREATE TABLE IF NOT EXISTS activation (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  email TEXT NOT NULL,
  machine_id TEXT NOT NULL,
  activated INTEGER DEFAULT 1,
  token TEXT,
  activated_at TEXT DEFAULT (datetime('now')),
  created_at TEXT DEFAULT (datetime('now')),
  updated_at TEXT DEFAULT (datetime('now'))
);

-- Tabela de Workspaces (Combs: ambientes isolados por worktree)
CREATE TABLE IF NOT EXISTS combs (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  name TEXT NOT NULL,
  description TEXT,
  base_branch TEXT NOT NULL DEFAULT 'main',
  branch TEXT,
  worktree_path TEXT,
  review_targets TEXT,
  status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'ready_for_review', 'applied', 'discarded', 'archived', 'error')),
  is_pinned INTEGER DEFAULT 0,
  pinned_at TEXT,
  last_opened_at TEXT,
  created_at TEXT DEFAULT (datetime('now')),
  updated_at TEXT DEFAULT (datetime('now')),
  FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

-- Tabela de Panes (terminais e agents dentro de um Workspace)
CREATE TABLE IF NOT EXISTS panes (
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

-- Índices para performance
CREATE INDEX IF NOT EXISTS idx_projects_last_opened ON projects(last_opened_at DESC);
CREATE INDEX IF NOT EXISTS idx_combs_project ON combs(project_id);
CREATE INDEX IF NOT EXISTS idx_combs_status ON combs(status);
CREATE INDEX IF NOT EXISTS idx_combs_last_opened ON combs(last_opened_at DESC);
CREATE INDEX IF NOT EXISTS idx_combs_pinned ON combs(is_pinned DESC, pinned_at DESC);
CREATE INDEX IF NOT EXISTS idx_panes_comb ON panes(comb_id);
CREATE INDEX IF NOT EXISTS idx_panes_layout ON panes(comb_id, layout_order);

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

CREATE TRIGGER IF NOT EXISTS update_combs_timestamp
AFTER UPDATE ON combs
BEGIN
  UPDATE combs SET updated_at = datetime('now') WHERE id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS update_panes_timestamp
AFTER UPDATE ON panes
BEGIN
  UPDATE panes SET updated_at = datetime('now') WHERE id = NEW.id;
END;
