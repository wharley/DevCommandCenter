"use strict";
// Dev Command Center - Mission Logs Repository
// Camada de acesso aos logs de missões (histórico de interações)
Object.defineProperty(exports, "__esModule", { value: true });
exports.MissionLogsRepository = void 0;
const connection_1 = require("../connection");
function rowToMissionLog(row) {
    return {
        id: row.id,
        missionId: row.mission_id,
        type: row.type,
        content: row.content,
        metadata: row.metadata ? JSON.parse(row.metadata) : null,
        createdAt: new Date(row.created_at),
    };
}
// ============================================
// Repository
// ============================================
exports.MissionLogsRepository = {
    /**
     * Lista logs de uma missão com filtros.
     */
    findAll(options) {
        const db = (0, connection_1.getDatabase)();
        const { missionId, type, limit = 100, offset = 0 } = options;
        const conditions = ['mission_id = ?'];
        const values = [missionId];
        if (type) {
            if (Array.isArray(type)) {
                conditions.push(`type IN (${type.map(() => '?').join(', ')})`);
                values.push(...type);
            }
            else {
                conditions.push('type = ?');
                values.push(type);
            }
        }
        values.push(limit, offset);
        const stmt = db.prepare(`
      SELECT * FROM mission_logs
      WHERE ${conditions.join(' AND ')}
      ORDER BY created_at ASC
      LIMIT ? OFFSET ?
    `);
        const rows = stmt.all(...values);
        return rows.map(rowToMissionLog);
    },
    /**
     * Lista todos os logs de uma missão.
     */
    findByMission(missionId) {
        return this.findAll({ missionId, limit: 1000 });
    },
    /**
     * Busca logs recentes de uma missão.
     */
    findRecent(missionId, limit = 20) {
        const db = (0, connection_1.getDatabase)();
        const stmt = db.prepare(`
      SELECT * FROM mission_logs
      WHERE mission_id = ?
      ORDER BY created_at DESC
      LIMIT ?
    `);
        const rows = stmt.all(missionId, limit);
        // Inverte para ordem cronológica
        return rows.map(rowToMissionLog).reverse();
    },
    /**
     * Busca um log por ID.
     */
    findById(id) {
        const db = (0, connection_1.getDatabase)();
        const stmt = db.prepare('SELECT * FROM mission_logs WHERE id = ?');
        const row = stmt.get(id);
        return row ? rowToMissionLog(row) : null;
    },
    /**
     * Cria um novo log.
     */
    create(data) {
        const db = (0, connection_1.getDatabase)();
        const id = (0, connection_1.generateId)();
        const stmt = db.prepare(`
      INSERT INTO mission_logs (id, mission_id, type, content, metadata)
      VALUES (?, ?, ?, ?, ?)
    `);
        stmt.run(id, data.missionId, data.type, data.content, data.metadata ? JSON.stringify(data.metadata) : null);
        return this.findById(id);
    },
    /**
     * Cria múltiplos logs de uma vez (batch insert).
     */
    createMany(logs) {
        const db = (0, connection_1.getDatabase)();
        const stmt = db.prepare(`
      INSERT INTO mission_logs (id, mission_id, type, content, metadata)
      VALUES (?, ?, ?, ?, ?)
    `);
        const insertMany = db.transaction((items) => {
            const ids = [];
            for (const item of items) {
                const id = (0, connection_1.generateId)();
                stmt.run(id, item.missionId, item.type, item.content, item.metadata ? JSON.stringify(item.metadata) : null);
                ids.push(id);
            }
            return ids;
        });
        const ids = insertMany(logs);
        return ids.map((id) => this.findById(id));
    },
    /**
     * Adiciona um log de informação.
     */
    logInfo(missionId, content, metadata) {
        return this.create({ missionId, type: 'info', content, metadata });
    },
    /**
     * Adiciona um log de prompt enviado.
     */
    logPrompt(missionId, content, metadata) {
        return this.create({ missionId, type: 'prompt', content, metadata });
    },
    /**
     * Adiciona um log de resposta recebida.
     */
    logResponse(missionId, content, metadata) {
        return this.create({ missionId, type: 'response', content, metadata });
    },
    /**
     * Adiciona um log de erro.
     */
    logError(missionId, content, metadata) {
        return this.create({ missionId, type: 'error', content, metadata });
    },
    /**
     * Adiciona um log de ação.
     */
    logAction(missionId, content, metadata) {
        return this.create({ missionId, type: 'action', content, metadata });
    },
    /**
     * Adiciona um log de input do usuário.
     */
    logUserInput(missionId, content, metadata) {
        return this.create({ missionId, type: 'user_input', content, metadata });
    },
    /**
     * Remove um log específico.
     */
    delete(id) {
        const db = (0, connection_1.getDatabase)();
        const stmt = db.prepare('DELETE FROM mission_logs WHERE id = ?');
        const result = stmt.run(id);
        return result.changes > 0;
    },
    /**
     * Remove todos os logs de uma missão.
     */
    deleteByMission(missionId) {
        const db = (0, connection_1.getDatabase)();
        const stmt = db.prepare('DELETE FROM mission_logs WHERE mission_id = ?');
        const result = stmt.run(missionId);
        return result.changes;
    },
    /**
     * Conta logs de uma missão.
     */
    countByMission(missionId) {
        const db = (0, connection_1.getDatabase)();
        const stmt = db.prepare('SELECT COUNT(*) as count FROM mission_logs WHERE mission_id = ?');
        const row = stmt.get(missionId);
        return row.count;
    },
    /**
     * Conta logs por tipo em uma missão.
     */
    countByType(missionId) {
        const db = (0, connection_1.getDatabase)();
        const stmt = db.prepare(`
      SELECT type, COUNT(*) as count
      FROM mission_logs
      WHERE mission_id = ?
      GROUP BY type
    `);
        const rows = stmt.all(missionId);
        const counts = {
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
        return counts;
    },
    /**
     * Busca o último log de um tipo específico.
     */
    findLastByType(missionId, type) {
        const db = (0, connection_1.getDatabase)();
        const stmt = db.prepare(`
      SELECT * FROM mission_logs
      WHERE mission_id = ? AND type = ?
      ORDER BY created_at DESC
      LIMIT 1
    `);
        const row = stmt.get(missionId, type);
        return row ? rowToMissionLog(row) : null;
    },
    /**
     * Calcula estatísticas de uso (tokens, tempo, etc.) de uma missão.
     */
    getUsageStats(missionId) {
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
};
//# sourceMappingURL=mission-logs.js.map