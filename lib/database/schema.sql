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
  plan_provider_id TEXT,
  code_provider_id TEXT,
  title TEXT NOT NULL,
  description TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'created' CHECK (status IN ('created', 'planning', 'plan_generated', 'generating_code', 'code_ready', 'applying', 'completed', 'failed', 'cancelled')),
  mission_type TEXT DEFAULT 'implementation' CHECK (mission_type IN ('implementation', 'analysis', 'agents_cli')),
  plan TEXT, -- JSON com o plano de ação gerado
  generated_code TEXT, -- JSON com as sugestões de código/diffs
  context TEXT, -- JSON com contexto do repo usado na missão
  preserve_instructions TEXT, -- texto livre: o que não alterar (ex.: "Não altere a seção Preview ao vivo")
  error_message TEXT,
  code_generation_attempts INTEGER DEFAULT 0, -- Contador de tentativas de geração de código
  is_committed INTEGER DEFAULT 0, -- Se as alterações já foram commitadas
  is_pushed INTEGER DEFAULT 0, -- Se o commit já foi enviado ao remoto
  pending_commands TEXT, -- JSON com comandos pendentes que o usuário precisa executar
  worktree_path TEXT, -- Path do worktree Git da missão (pipeline paralelo)
  worktree_branch TEXT, -- Branch do worktree (ex.: dcc-mission-<id>)
  base_branch TEXT, -- Branch de origem ao criar worktree (agents_cli)
  target_branch TEXT, -- Branch de destino após aplicar patch (agents_cli)
  last_output_summary TEXT, -- Resumo da saída do terminal para o card (agents_cli)
  last_git_summary TEXT, -- JSON { changedFiles, insertions, deletions } (agents_cli)
  wall_status TEXT, -- running | ready_for_review | applied | discarded | canceled | error | apply_failed (agents_cli)
  started_at TEXT,
  completed_at TEXT,
  created_at TEXT DEFAULT (datetime('now')),
  updated_at TEXT DEFAULT (datetime('now')),
  FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
  FOREIGN KEY (provider_id) REFERENCES providers(id) ON DELETE SET NULL,
  FOREIGN KEY (plan_provider_id) REFERENCES providers(id) ON DELETE SET NULL,
  FOREIGN KEY (code_provider_id) REFERENCES providers(id) ON DELETE SET NULL
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

-- Tabela de Combs (workspaces isolados por worktree dentro de um projeto)
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
  last_opened_at TEXT,
  created_at TEXT DEFAULT (datetime('now')),
  updated_at TEXT DEFAULT (datetime('now')),
  FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

-- Tabela de Panes (terminais e agents dentro de um Comb)
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
CREATE INDEX IF NOT EXISTS idx_missions_project ON missions(project_id);
CREATE INDEX IF NOT EXISTS idx_missions_status ON missions(status);
CREATE INDEX IF NOT EXISTS idx_missions_created ON missions(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_mission_logs_mission ON mission_logs(mission_id);
CREATE INDEX IF NOT EXISTS idx_mission_logs_created ON mission_logs(created_at DESC);

CREATE INDEX IF NOT EXISTS idx_combs_project ON combs(project_id);
CREATE INDEX IF NOT EXISTS idx_combs_status ON combs(status);
CREATE INDEX IF NOT EXISTS idx_combs_last_opened ON combs(last_opened_at DESC);
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

CREATE TRIGGER IF NOT EXISTS update_missions_timestamp
AFTER UPDATE ON missions
BEGIN
  UPDATE missions SET updated_at = datetime('now') WHERE id = NEW.id;
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
