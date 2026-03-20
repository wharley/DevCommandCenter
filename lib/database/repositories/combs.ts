import { getDatabase, generateId } from '../connection';
import type {
  Comb,
  CombStatus,
  CreateCombDTO,
  UpdateCombDTO,
  CombsQueryOptions,
} from '../types';

interface CombRow {
  id: string;
  project_id: string;
  name: string;
  description: string | null;
  base_branch: string;
  branch: string | null;
  worktree_path: string | null;
  status: string;
  last_opened_at: string | null;
  created_at: string;
  updated_at: string;
}

function rowToComb(row: CombRow): Comb {
  return {
    id: row.id,
    projectId: row.project_id,
    name: row.name,
    description: row.description ?? null,
    baseBranch: row.base_branch,
    branch: row.branch ?? null,
    worktreePath: row.worktree_path ?? null,
    status: row.status as CombStatus,
    lastOpenedAt: row.last_opened_at ? new Date(row.last_opened_at) : null,
    createdAt: new Date(row.created_at),
    updatedAt: new Date(row.updated_at),
  };
}

export const CombsRepository = {
  findAll(options: CombsQueryOptions = {}): Comb[] {
    const db = getDatabase();
    const {
      limit = 100,
      offset = 0,
      projectId,
      status,
      orderBy = 'createdAt',
      orderDirection = 'desc',
    } = options;

    const conditions: string[] = [];
    const values: (string | number)[] = [];

    if (projectId) {
      conditions.push('project_id = ?');
      values.push(projectId);
    }

    if (status) {
      if (Array.isArray(status)) {
        conditions.push(`status IN (${status.map(() => '?').join(', ')})`);
        values.push(...status);
      } else {
        conditions.push('status = ?');
        values.push(status);
      }
    }

    const whereClause = conditions.length > 0
      ? `WHERE ${conditions.join(' AND ')}`
      : '';

    const columnMap: Record<string, string> = {
      name: 'name',
      status: 'status',
      createdAt: 'created_at',
      updatedAt: 'updated_at',
      lastOpenedAt: 'last_opened_at',
    };

    const orderColumn = columnMap[orderBy] || 'created_at';
    const direction = orderDirection.toUpperCase() === 'ASC' ? 'ASC' : 'DESC';

    values.push(limit, offset);

    const stmt = db.prepare(`
      SELECT * FROM combs
      ${whereClause}
      ORDER BY ${orderColumn} ${direction}
      LIMIT ? OFFSET ?
    `);

    const rows = stmt.all(...values) as CombRow[];
    return rows.map(rowToComb);
  },

  findByProject(projectId: string, limit: number = 50): Comb[] {
    return this.findAll({ projectId, limit, orderBy: 'lastOpenedAt', orderDirection: 'desc' });
  },

  findById(id: string): Comb | undefined {
    const db = getDatabase();
    const row = db.prepare('SELECT * FROM combs WHERE id = ?').get(id) as CombRow | undefined;
    return row ? rowToComb(row) : undefined;
  },

  create(data: CreateCombDTO): Comb {
    const db = getDatabase();
    const id = generateId();
    const now = new Date().toISOString();

    db.prepare(`
      INSERT INTO combs (id, project_id, name, description, base_branch, status, last_opened_at, created_at, updated_at)
      VALUES (?, ?, ?, ?, ?, 'active', ?, ?, ?)
    `).run(
      id,
      data.projectId,
      data.name,
      data.description ?? null,
      data.baseBranch,
      now,
      now,
      now,
    );

    return this.findById(id)!;
  },

  update(id: string, data: UpdateCombDTO): Comb | undefined {
    const db = getDatabase();
    const updates: string[] = [];
    const values: (string | number | null)[] = [];

    if (data.name !== undefined) {
      updates.push('name = ?');
      values.push(data.name);
    }
    if (data.description !== undefined) {
      updates.push('description = ?');
      values.push(data.description);
    }
    if (data.branch !== undefined) {
      updates.push('branch = ?');
      values.push(data.branch);
    }
    if (data.worktreePath !== undefined) {
      updates.push('worktree_path = ?');
      values.push(data.worktreePath);
    }
    if (data.status !== undefined) {
      updates.push('status = ?');
      values.push(data.status);
    }
    if (data.lastOpenedAt !== undefined) {
      updates.push('last_opened_at = ?');
      values.push(data.lastOpenedAt.toISOString());
    }

    if (updates.length === 0) return this.findById(id);

    values.push(id);
    db.prepare(`UPDATE combs SET ${updates.join(', ')} WHERE id = ?`).run(...values);
    return this.findById(id);
  },

  delete(id: string): boolean {
    const db = getDatabase();
    const result = db.prepare('DELETE FROM combs WHERE id = ?').run(id);
    return result.changes > 0;
  },

  touchLastOpened(id: string): void {
    const db = getDatabase();
    db.prepare('UPDATE combs SET last_opened_at = datetime(\'now\') WHERE id = ?').run(id);
  },
};
