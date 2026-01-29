// Dev Command Center - Mission Logs Repository
// Camada de acesso aos logs de missões (histórico de interações)

import { getDatabase, generateId } from "../connection";
import type {
  MissionLog,
  MissionLogType,
  MissionLogMetadata,
  CreateMissionLogDTO,
  MissionLogsQueryOptions,
} from "../types";

// ============================================
// Helpers de conversão
// ============================================

interface MissionLogRow {
  id: string;
  mission_id: string;
  type: string;
  content: string;
  metadata: string | null;
  created_at: string;
}

/**
 * Converte created_at do SQLite para Date (instant correto).
 * - Valores em ISO com timezone (Z ou +/-) são interpretados como estão.
 * - Valores "YYYY-MM-DD HH:MM:SS" sem timezone: tratamos como UTC para que
 *   a UI (format em hora local) exiba o horário certo.
 * - Inserções novas usam sempre UTC via toISOString().
 */
function parseCreatedAt(createdAt: string): Date {
  if (!createdAt) return new Date();
  const trimmed = createdAt.trim();
  if (/[Z+-]\d{2}:?\d{2}$/.test(trimmed)) return new Date(trimmed);
  const iso = trimmed.replace(" ", "T") + "Z";
  return new Date(iso);
}

/** Retorna data/hora atual em UTC para gravar no banco (evita datetime('now') local). */
function nowUtcIso(): string {
  return new Date().toISOString();
}

function rowToMissionLog(row: MissionLogRow): MissionLog {
  return {
    id: row.id,
    missionId: row.mission_id,
    type: row.type as MissionLogType,
    content: row.content,
    metadata: row.metadata ? JSON.parse(row.metadata) : null,
    createdAt: parseCreatedAt(row.created_at),
  };
}

// ============================================
// Repository
// ============================================

export const MissionLogsRepository = {
  /**
   * Lista logs de uma missão com filtros.
   */
  findAll(options: MissionLogsQueryOptions): MissionLog[] {
    const db = getDatabase();
    const { missionId, type, limit = 100, offset = 0 } = options;

    if (!missionId) {
      // Se não tiver missionId, retorna todos os logs (cuidado com performance)
      const stmt = db.prepare(`
        SELECT * FROM mission_logs
        ORDER BY created_at DESC, rowid DESC
        LIMIT ? OFFSET ?
      `);
      const rows = stmt.all(limit, offset) as MissionLogRow[];
      return rows.map(rowToMissionLog);
    }

    const conditions: string[] = ["mission_id = ?"];
    const values: (string | number)[] = [missionId];

    if (type) {
      if (Array.isArray(type)) {
        conditions.push(`type IN (${type.map(() => "?").join(", ")})`);
        values.push(...type);
      } else {
        conditions.push("type = ?");
        values.push(type);
      }
    }

    values.push(limit, offset);

    const stmt = db.prepare(`
      SELECT * FROM mission_logs
      WHERE ${conditions.join(" AND ")}
      ORDER BY created_at DESC, rowid DESC
      LIMIT ? OFFSET ?
    `);

    const rows = stmt.all(...values) as MissionLogRow[];
    return rows.map(rowToMissionLog);
  },

  /**
   * Lista todos os logs de uma missão.
   */
  findByMission(
    missionId: string,
    limit?: number,
    offset?: number,
  ): MissionLog[] {
    return this.findAll({
      missionId,
      limit: limit || 1000,
      offset: offset || 0,
    });
  },

  /**
   * Busca logs recentes de uma missão.
   */
  findRecent(missionId: string, limit: number = 20): MissionLog[] {
    const db = getDatabase();

    const stmt = db.prepare(`
      SELECT * FROM mission_logs
      WHERE mission_id = ?
      ORDER BY created_at DESC, rowid DESC
      LIMIT ?
    `);

    const rows = stmt.all(missionId, limit) as MissionLogRow[];
    // Inverte para ordem cronológica
    return rows.map(rowToMissionLog).reverse();
  },

  /**
   * Busca um log por ID.
   */
  findById(id: string): MissionLog | null {
    const db = getDatabase();

    const stmt = db.prepare("SELECT * FROM mission_logs WHERE id = ?");
    const row = stmt.get(id) as MissionLogRow | undefined;

    return row ? rowToMissionLog(row) : null;
  },

  /**
   * Cria um novo log.
   */
  create(data: CreateMissionLogDTO): MissionLog {
    const db = getDatabase();
    const id = generateId();
    const createdAt = nowUtcIso();

    const stmt = db.prepare(`
      INSERT INTO mission_logs (id, mission_id, type, content, metadata, created_at)
      VALUES (?, ?, ?, ?, ?, ?)
    `);

    stmt.run(
      id,
      data.missionId,
      data.type,
      data.content,
      data.metadata ? JSON.stringify(data.metadata) : null,
      createdAt,
    );

    return this.findById(id)!;
  },

  /**
   * Cria múltiplos logs de uma vez (batch insert).
   */
  createMany(logs: CreateMissionLogDTO[]): MissionLog[] {
    const db = getDatabase();

    const stmt = db.prepare(`
      INSERT INTO mission_logs (id, mission_id, type, content, metadata, created_at)
      VALUES (?, ?, ?, ?, ?, ?)
    `);

    const insertMany = db.transaction((items: CreateMissionLogDTO[]) => {
      const ids: string[] = [];
      for (const item of items) {
        const id = generateId();
        stmt.run(
          id,
          item.missionId,
          item.type,
          item.content,
          item.metadata ? JSON.stringify(item.metadata) : null,
          nowUtcIso(),
        );
        ids.push(id);
      }
      return ids;
    });

    const ids = insertMany(logs);
    return ids.map((id) => this.findById(id)!);
  },

  /**
   * Adiciona um log de informação.
   */
  logInfo(
    missionId: string,
    content: string,
    metadata?: MissionLogMetadata,
  ): MissionLog {
    return this.create({ missionId, type: "info", content, metadata });
  },

  /**
   * Adiciona um log de prompt enviado.
   */
  logPrompt(
    missionId: string,
    content: string,
    metadata?: MissionLogMetadata,
  ): MissionLog {
    return this.create({ missionId, type: "prompt", content, metadata });
  },

  /**
   * Adiciona um log de resposta recebida.
   */
  logResponse(
    missionId: string,
    content: string,
    metadata?: MissionLogMetadata,
  ): MissionLog {
    return this.create({ missionId, type: "response", content, metadata });
  },

  /**
   * Adiciona um log de erro.
   */
  logError(
    missionId: string,
    content: string,
    metadata?: MissionLogMetadata,
  ): MissionLog {
    return this.create({ missionId, type: "error", content, metadata });
  },

  /**
   * Adiciona um log de ação.
   */
  logAction(
    missionId: string,
    content: string,
    metadata?: MissionLogMetadata,
  ): MissionLog {
    return this.create({ missionId, type: "action", content, metadata });
  },

  /**
   * Adiciona um log de input do usuário.
   */
  logUserInput(
    missionId: string,
    content: string,
    metadata?: MissionLogMetadata,
  ): MissionLog {
    return this.create({ missionId, type: "user_input", content, metadata });
  },

  /**
   * Remove um log específico.
   */
  delete(id: string): boolean {
    const db = getDatabase();

    const stmt = db.prepare("DELETE FROM mission_logs WHERE id = ?");
    const result = stmt.run(id);

    return result.changes > 0;
  },

  /**
   * Remove todos os logs de uma missão.
   */
  deleteByMission(missionId: string): number {
    const db = getDatabase();

    const stmt = db.prepare("DELETE FROM mission_logs WHERE mission_id = ?");
    const result = stmt.run(missionId);

    return result.changes;
  },

  /**
   * Conta logs de uma missão.
   */
  countByMission(missionId: string): number {
    const db = getDatabase();

    const stmt = db.prepare(
      "SELECT COUNT(*) as count FROM mission_logs WHERE mission_id = ?",
    );
    const row = stmt.get(missionId) as { count: number };

    return row.count;
  },

  /**
   * Conta logs por tipo em uma missão.
   */
  countByType(missionId: string): Record<MissionLogType, number> {
    const db = getDatabase();

    const stmt = db.prepare(`
      SELECT type, COUNT(*) as count
      FROM mission_logs
      WHERE mission_id = ?
      GROUP BY type
    `);

    const rows = stmt.all(missionId) as { type: string; count: number }[];

    const counts: Record<string, number> = {
      info: 0,
      prompt: 0,
      response: 0,
      error: 0,
      action: 0,
      user_input: 0,
    };

    for (const row of rows) {
      counts[row.type] = row.count;
    }

    return counts as Record<MissionLogType, number>;
  },

  /**
   * Busca o último log de um tipo específico.
   */
  findLastByType(missionId: string, type: MissionLogType): MissionLog | null {
    const db = getDatabase();

    const stmt = db.prepare(`
      SELECT * FROM mission_logs
      WHERE mission_id = ? AND type = ?
      ORDER BY created_at DESC, rowid DESC
      LIMIT 1
    `);

    const row = stmt.get(missionId, type) as MissionLogRow | undefined;

    return row ? rowToMissionLog(row) : null;
  },

  /**
   * Calcula estatísticas de uso (tokens, tempo, etc.) de uma missão.
   */
  getUsageStats(missionId: string): {
    totalTokens: number;
    totalDurationMs: number;
  } {
    const logs = this.findByMission(missionId);

    let totalTokens = 0;
    let totalDurationMs = 0;

    for (const log of logs) {
      if (log.metadata) {
        if (log.metadata.tokensUsed) {
          totalTokens += log.metadata.tokensUsed;
        }
        if (log.metadata.durationMs) {
          totalDurationMs += log.metadata.durationMs;
        }
      }
    }

    return { totalTokens, totalDurationMs };
  },

  /**
   * Busca logs por nível/tipo.
   */
  findByLevel(level: MissionLogType, missionId?: string): MissionLog[] {
    const db = getDatabase();

    if (missionId) {
      const stmt = db.prepare(`
        SELECT * FROM mission_logs
        WHERE type = ? AND mission_id = ?
        ORDER BY created_at ASC, rowid ASC
      `);
      const rows = stmt.all(level, missionId) as MissionLogRow[];
      return rows.map(rowToMissionLog);
    }

    const stmt = db.prepare(`
      SELECT * FROM mission_logs
      WHERE type = ?
      ORDER BY created_at DESC, rowid DESC
      LIMIT 100
    `);
    const rows = stmt.all(level) as MissionLogRow[];
    return rows.map(rowToMissionLog);
  },

  /**
   * Busca logs por texto.
   */
  search(query: string, missionId?: string): MissionLog[] {
    const db = getDatabase();
    const searchTerm = `%${query}%`;

    if (missionId) {
      const stmt = db.prepare(`
        SELECT * FROM mission_logs
        WHERE content LIKE ? AND mission_id = ?
        ORDER BY created_at ASC, rowid ASC
      `);
      const rows = stmt.all(searchTerm, missionId) as MissionLogRow[];
      return rows.map(rowToMissionLog);
    }

    const stmt = db.prepare(`
      SELECT * FROM mission_logs
      WHERE content LIKE ?
      ORDER BY created_at DESC, rowid DESC
      LIMIT 100
    `);
    const rows = stmt.all(searchTerm) as MissionLogRow[];
    return rows.map(rowToMissionLog);
  },

  /**
   * Adiciona um log de aviso.
   */
  logWarning(
    missionId: string,
    content: string,
    metadata?: MissionLogMetadata,
  ): MissionLog {
    return this.create({
      missionId,
      type: "info",
      content: `[WARNING] ${content}`,
      metadata,
    });
  },

  /**
   * Adiciona um log de debug.
   */
  logDebug(
    missionId: string,
    content: string,
    metadata?: MissionLogMetadata,
  ): MissionLog {
    return this.create({
      missionId,
      type: "info",
      content: `[DEBUG] ${content}`,
      metadata,
    });
  },

  /**
   * Adiciona um log de ação do agente.
   */
  logAgentAction(
    missionId: string,
    action: string,
    details?: MissionLogMetadata,
  ): MissionLog {
    return this.logAction(missionId, action, details);
  },

  /**
   * Obtém estatísticas de logs de uma missão.
   */
  getStats(missionId: string): {
    total: number;
    byType: Record<MissionLogType, number>;
  } {
    const total = this.countByMission(missionId);
    const byType = this.countByType(missionId);

    return { total, byType };
  },

  /**
   * Obtém os logs mais recentes de uma missão.
   */
  getLatest(missionId: string, count: number = 10): MissionLog[] {
    return this.findRecent(missionId, count);
  },
};
