// Dev Command Center - Projects Repository
// Camada de acesso aos dados de projetos

import { getDatabase, generateId } from '../connection';
import type {
  Project,
  CreateProjectDTO,
  UpdateProjectDTO,
  ProjectsQueryOptions,
} from '../types';

// ============================================
// Helpers de conversão
// ============================================

interface ProjectRow {
  id: string;
  name: string;
  path: string;
  description: string | null;
  default_provider_id: string | null;
  git_remote_url: string | null;
  repo_config: string | null;
  last_opened_at: string | null;
  created_at: string;
  updated_at: string;
}

function parseRepoConfigJson(raw: string | null | undefined) {
  if (!raw?.trim()) return null;
  try {
    return JSON.parse(raw);
  } catch {
    return null;
  }
}

function rowToProject(row: ProjectRow): Project {
  return {
    id: row.id,
    name: row.name,
    path: row.path,
    description: row.description,
    defaultProviderId: row.default_provider_id,
    gitRemoteUrl: row.git_remote_url,
    repoConfig: parseRepoConfigJson(row.repo_config),
    lastOpenedAt: row.last_opened_at ? new Date(row.last_opened_at) : null,
    createdAt: new Date(row.created_at),
    updatedAt: new Date(row.updated_at),
  };
}

// ============================================
// Repository
// ============================================

export const ProjectsRepository = {
  /**
   * Lista todos os projetos.
   */
  findAll(options: ProjectsQueryOptions = {}): Project[] {
    const db = getDatabase();
    const {
      limit = 100,
      offset = 0,
      orderBy = 'lastOpenedAt',
      orderDirection = 'desc',
    } = options;
    
    const columnMap: Record<string, string> = {
      name: 'name',
      lastOpenedAt: 'last_opened_at',
      createdAt: 'created_at',
    };
    
    const orderColumn = columnMap[orderBy] || 'last_opened_at';
    const direction = orderDirection.toUpperCase() === 'ASC' ? 'ASC' : 'DESC';
    
    // Tratamento especial para ordenação por last_opened_at (NULLs por último)
    const orderClause = orderColumn === 'last_opened_at'
      ? `${orderColumn} IS NULL, ${orderColumn} ${direction}`
      : `${orderColumn} ${direction}`;
    
    const stmt = db.prepare(`
      SELECT * FROM projects
      ORDER BY ${orderClause}
      LIMIT ? OFFSET ?
    `);
    
    const rows = stmt.all(limit, offset) as ProjectRow[];
    return rows.map(rowToProject);
  },

  /**
   * Busca projetos recentes (últimos abertos).
   */
  findRecent(limit: number = 10): Project[] {
    const db = getDatabase();
    
    const stmt = db.prepare(`
      SELECT * FROM projects
      WHERE last_opened_at IS NOT NULL
      ORDER BY last_opened_at DESC
      LIMIT ?
    `);
    
    const rows = stmt.all(limit) as ProjectRow[];
    return rows.map(rowToProject);
  },

  /**
   * Busca um projeto por ID.
   */
  findById(id: string): Project | null {
    const db = getDatabase();
    
    const stmt = db.prepare('SELECT * FROM projects WHERE id = ?');
    const row = stmt.get(id) as ProjectRow | undefined;
    
    return row ? rowToProject(row) : null;
  },

  /**
   * Busca um projeto pelo caminho.
   */
  findByPath(path: string): Project | null {
    const db = getDatabase();
    
    const stmt = db.prepare('SELECT * FROM projects WHERE path = ?');
    const row = stmt.get(path) as ProjectRow | undefined;
    
    return row ? rowToProject(row) : null;
  },

  /**
   * Cria um novo projeto.
   */
  create(data: CreateProjectDTO): Project {
    const db = getDatabase();
    const id = generateId();
    
    const stmt = db.prepare(`
      INSERT INTO projects (id, name, path, description, default_provider_id, git_remote_url, repo_config)
      VALUES (?, ?, ?, ?, ?, ?, ?)
    `);
    
    stmt.run(
      id,
      data.name,
      data.path,
      data.description || null,
      data.defaultProviderId || null,
      data.gitRemoteUrl || null,
      data.repoConfig ? JSON.stringify(data.repoConfig) : null
    );
    
    return this.findById(id)!;
  },

  /**
   * Atualiza um projeto existente.
   */
  update(id: string, data: UpdateProjectDTO): Project | null {
    const db = getDatabase();
    const existing = this.findById(id);
    
    if (!existing) {
      return null;
    }
    
    const updates: string[] = [];
    const values: (string | null)[] = [];
    
    if (data.name !== undefined) {
      updates.push('name = ?');
      values.push(data.name);
    }
    if (data.description !== undefined) {
      updates.push('description = ?');
      values.push(data.description);
    }
    if (data.defaultProviderId !== undefined) {
      updates.push('default_provider_id = ?');
      values.push(data.defaultProviderId);
    }
    if (data.gitRemoteUrl !== undefined) {
      updates.push('git_remote_url = ?');
      values.push(data.gitRemoteUrl);
    }
    if (data.repoConfig !== undefined) {
      updates.push('repo_config = ?');
      values.push(data.repoConfig ? JSON.stringify(data.repoConfig) : null);
    }
    
    if (updates.length === 0) {
      return existing;
    }
    
    values.push(id);
    
    const stmt = db.prepare(`
      UPDATE projects
      SET ${updates.join(', ')}
      WHERE id = ?
    `);
    
    stmt.run(...values);
    
    return this.findById(id);
  },

  /**
   * Atualiza o timestamp de último acesso.
   */
  updateLastOpened(id: string): boolean {
    const db = getDatabase();
    
    const stmt = db.prepare(`
      UPDATE projects
      SET last_opened_at = datetime('now')
      WHERE id = ?
    `);
    
    const result = stmt.run(id);
    return result.changes > 0;
  },

  /**
   * Remove um projeto (e suas missões em cascata).
   */
  delete(id: string): boolean {
    const db = getDatabase();
    
    const stmt = db.prepare('DELETE FROM projects WHERE id = ?');
    const result = stmt.run(id);
    
    return result.changes > 0;
  },

  /**
   * Conta o total de projetos.
   */
  count(): number {
    const db = getDatabase();
    
    const stmt = db.prepare('SELECT COUNT(*) as count FROM projects');
    const row = stmt.get() as { count: number };
    
    return row.count;
  },

  /**
   * Verifica se um projeto com o caminho já existe.
   */
  existsByPath(path: string): boolean {
    const db = getDatabase();
    
    const stmt = db.prepare('SELECT 1 FROM projects WHERE path = ? LIMIT 1');
    const row = stmt.get(path);
    
    return row !== undefined;
  },

  /**
   * Busca projetos por texto (nome ou descrição).
   */
  search(query: string, limit: number = 20): Project[] {
    const db = getDatabase();
    const searchTerm = `%${query}%`;
    
    const stmt = db.prepare(`
      SELECT * FROM projects
      WHERE name LIKE ? OR description LIKE ? OR path LIKE ?
      ORDER BY last_opened_at DESC NULLS LAST
      LIMIT ?
    `);
    
    const rows = stmt.all(searchTerm, searchTerm, searchTerm, limit) as ProjectRow[];
    return rows.map(rowToProject);
  },

  /**
   * Obtém estatísticas de um projeto.
   */
  getStats(id: string): { totalMissions: number; completedMissions: number; activeMissions: number; failedMissions: number } | null {
    const db = getDatabase();
    
    const project = this.findById(id);
    if (!project) {
      return null;
    }
    
    const stmt = db.prepare(`
      SELECT 
        COUNT(*) as total,
        SUM(CASE WHEN status = 'completed' THEN 1 ELSE 0 END) as completed,
        SUM(CASE WHEN status IN ('created', 'planning', 'plan_generated', 'generating_code', 'code_ready', 'applying') THEN 1 ELSE 0 END) as active,
        SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END) as failed
      FROM missions
      WHERE project_id = ?
    `);
    
    const row = stmt.get(id) as { total: number; completed: number; active: number; failed: number };
    
    return {
      totalMissions: row.total || 0,
      completedMissions: row.completed || 0,
      activeMissions: row.active || 0,
      failedMissions: row.failed || 0,
    };
  },
};
