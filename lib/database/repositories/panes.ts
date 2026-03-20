import { getDatabase, generateId } from '../connection';
import type {
  Pane,
  PaneType,
  PaneStatus,
  CreatePaneDTO,
  UpdatePaneDTO,
  PanesQueryOptions,
} from '../types';

interface PaneRow {
  id: string;
  comb_id: string;
  type: string;
  provider_id: string | null;
  title: string | null;
  initial_prompt: string | null;
  cwd: string | null;
  pty_owner_key: string | null;
  status: string;
  layout_order: number;
  last_activity_at: string | null;
  created_at: string;
  updated_at: string;
}

function rowToPane(row: PaneRow): Pane {
  return {
    id: row.id,
    combId: row.comb_id,
    type: row.type as PaneType,
    providerId: row.provider_id ?? null,
    title: row.title ?? null,
    initialPrompt: row.initial_prompt ?? null,
    cwd: row.cwd ?? null,
    ptyOwnerKey: row.pty_owner_key ?? null,
    status: row.status as PaneStatus,
    layoutOrder: row.layout_order,
    lastActivityAt: row.last_activity_at ? new Date(row.last_activity_at) : null,
    createdAt: new Date(row.created_at),
    updatedAt: new Date(row.updated_at),
  };
}

export const PanesRepository = {
  findAll(options: PanesQueryOptions = {}): Pane[] {
    const db = getDatabase();
    const {
      limit = 100,
      offset = 0,
      combId,
      type,
      orderBy = 'layoutOrder',
      orderDirection = 'asc',
    } = options;

    const conditions: string[] = [];
    const values: (string | number)[] = [];

    if (combId) {
      conditions.push('comb_id = ?');
      values.push(combId);
    }

    if (type) {
      conditions.push('type = ?');
      values.push(type);
    }

    const whereClause = conditions.length > 0
      ? `WHERE ${conditions.join(' AND ')}`
      : '';

    const columnMap: Record<string, string> = {
      layoutOrder: 'layout_order',
      createdAt: 'created_at',
      lastActivityAt: 'last_activity_at',
    };

    const orderColumn = columnMap[orderBy] || 'layout_order';
    const direction = orderDirection.toUpperCase() === 'ASC' ? 'ASC' : 'DESC';

    values.push(limit, offset);

    const stmt = db.prepare(`
      SELECT * FROM panes
      ${whereClause}
      ORDER BY ${orderColumn} ${direction}
      LIMIT ? OFFSET ?
    `);

    const rows = stmt.all(...values) as PaneRow[];
    return rows.map(rowToPane);
  },

  findByComb(combId: string): Pane[] {
    return this.findAll({ combId, orderBy: 'layoutOrder', orderDirection: 'asc' });
  },

  findById(id: string): Pane | undefined {
    const db = getDatabase();
    const row = db.prepare('SELECT * FROM panes WHERE id = ?').get(id) as PaneRow | undefined;
    return row ? rowToPane(row) : undefined;
  },

  create(data: CreatePaneDTO): Pane {
    const db = getDatabase();
    const id = generateId();
    const now = new Date().toISOString();

    const nextOrder = data.layoutOrder ?? this.getNextLayoutOrder(data.combId);

    db.prepare(`
      INSERT INTO panes (id, comb_id, type, provider_id, title, initial_prompt, status, layout_order, created_at, updated_at)
      VALUES (?, ?, ?, ?, ?, ?, 'idle', ?, ?, ?)
    `).run(
      id,
      data.combId,
      data.type,
      data.providerId ?? null,
      data.title ?? null,
      data.initialPrompt ?? null,
      nextOrder,
      now,
      now,
    );

    return this.findById(id)!;
  },

  update(id: string, data: UpdatePaneDTO): Pane | undefined {
    const db = getDatabase();
    const updates: string[] = [];
    const values: (string | number | null)[] = [];

    if (data.title !== undefined) {
      updates.push('title = ?');
      values.push(data.title);
    }
    if (data.initialPrompt !== undefined) {
      updates.push('initial_prompt = ?');
      values.push(data.initialPrompt);
    }
    if (data.providerId !== undefined) {
      updates.push('provider_id = ?');
      values.push(data.providerId);
    }
    if (data.cwd !== undefined) {
      updates.push('cwd = ?');
      values.push(data.cwd);
    }
    if (data.ptyOwnerKey !== undefined) {
      updates.push('pty_owner_key = ?');
      values.push(data.ptyOwnerKey);
    }
    if (data.status !== undefined) {
      updates.push('status = ?');
      values.push(data.status);
    }
    if (data.layoutOrder !== undefined) {
      updates.push('layout_order = ?');
      values.push(data.layoutOrder);
    }
    if (data.lastActivityAt !== undefined) {
      updates.push('last_activity_at = ?');
      values.push(data.lastActivityAt.toISOString());
    }

    if (updates.length === 0) return this.findById(id);

    values.push(id);
    db.prepare(`UPDATE panes SET ${updates.join(', ')} WHERE id = ?`).run(...values);
    return this.findById(id);
  },

  delete(id: string): boolean {
    const db = getDatabase();
    const result = db.prepare('DELETE FROM panes WHERE id = ?').run(id);
    return result.changes > 0;
  },

  deleteByComb(combId: string): number {
    const db = getDatabase();
    const result = db.prepare('DELETE FROM panes WHERE comb_id = ?').run(combId);
    return result.changes;
  },

  getNextLayoutOrder(combId: string): number {
    const db = getDatabase();
    const row = db.prepare(
      'SELECT COALESCE(MAX(layout_order), -1) + 1 AS next_order FROM panes WHERE comb_id = ?'
    ).get(combId) as { next_order: number } | undefined;
    return row?.next_order ?? 0;
  },
};
