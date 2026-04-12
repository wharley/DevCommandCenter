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

-- Histórico de terminal (scrollback) persistido por painel — gzip(JSON array de chunks UTF-8), reidratação após reinício da app
CREATE TABLE IF NOT EXISTS pane_terminal_scrollback (
  pane_id TEXT PRIMARY KEY,
  payload_z BLOB NOT NULL,
  updated_at TEXT DEFAULT (datetime('now')),
  FOREIGN KEY (pane_id) REFERENCES panes(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_pane_scrollback_updated ON pane_terminal_scrollback(updated_at DESC);

-- Tabela de estado do daemon (tasks agendadas + sessões persistentes no processo)
CREATE TABLE IF NOT EXISTS daemon_task_runs (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  task_id TEXT NOT NULL,
  task_name TEXT NOT NULL,
  command TEXT NOT NULL,
  schedule TEXT NOT NULL,
  cwd_mode TEXT NOT NULL DEFAULT 'project',
  enabled INTEGER DEFAULT 1,
  trigger_when TEXT,
  trigger_prompt TEXT,
  trigger_provider_id TEXT,
  status TEXT NOT NULL DEFAULT 'idle' CHECK (status IN ('idle', 'scheduled', 'running', 'waiting', 'completed', 'failed', 'disabled', 'skipped')),
  attached INTEGER DEFAULT 0,
  pty_id TEXT,
  pane_id TEXT,
  comb_id TEXT,
  next_run_at TEXT,
  last_run_at TEXT,
  last_exit_code INTEGER,
  last_output_excerpt TEXT,
  last_error TEXT,
  created_at TEXT DEFAULT (datetime('now')),
  updated_at TEXT DEFAULT (datetime('now')),
  UNIQUE(project_id, task_id)
);

CREATE TABLE IF NOT EXISTS daemon_rpc_requests (
  id TEXT PRIMARY KEY,
  method TEXT NOT NULL,
  params_json TEXT,
  status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'done', 'error')),
  response_json TEXT,
  error TEXT,
  created_at TEXT DEFAULT (datetime('now')),
  updated_at TEXT DEFAULT (datetime('now'))
);

-- Tabela de processos gerenciados (supervisor de longa duração)
CREATE TABLE IF NOT EXISTS daemon_processes (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  process_id TEXT NOT NULL,
  process_name TEXT NOT NULL,
  command TEXT NOT NULL,
  cwd_mode TEXT NOT NULL DEFAULT 'project',
  auto_restart INTEGER DEFAULT 1,
  status TEXT NOT NULL DEFAULT 'stopped' CHECK (status IN ('stopped', 'starting', 'running', 'stopping', 'restarting', 'crashed', 'failed')),
  pty_id TEXT,
  pane_id TEXT,
  comb_id TEXT,
  pid INTEGER,
  exit_code INTEGER,
  restart_count INTEGER DEFAULT 0,
  last_restart_at TEXT,
  backoff_seconds INTEGER DEFAULT 1,
  cpu_percent REAL DEFAULT 0.0,
  memory_mb REAL DEFAULT 0.0,
  last_metrics_at TEXT,
  last_output_excerpt TEXT,
  last_error TEXT,
  started_at TEXT,
  stopped_at TEXT,
  created_at TEXT DEFAULT (datetime('now')),
  updated_at TEXT DEFAULT (datetime('now')),
  UNIQUE(project_id, process_id),
  FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

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

CREATE TRIGGER IF NOT EXISTS update_daemon_task_runs_timestamp
AFTER UPDATE ON daemon_task_runs
BEGIN
  UPDATE daemon_task_runs SET updated_at = datetime('now') WHERE id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS update_daemon_rpc_requests_timestamp
AFTER UPDATE ON daemon_rpc_requests
BEGIN
  UPDATE daemon_rpc_requests SET updated_at = datetime('now') WHERE id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS update_daemon_processes_timestamp
AFTER UPDATE ON daemon_processes
BEGIN
  UPDATE daemon_processes SET updated_at = datetime('now') WHERE id = NEW.id;
END;
