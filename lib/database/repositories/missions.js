"use strict";
// Dev Command Center - Missions Repository
// Camada de acesso aos dados de missões de código
Object.defineProperty(exports, "__esModule", { value: true });
exports.MissionsRepository = void 0;
const connection_1 = require("../connection");
function rowToMission(row) {
    return {
        id: row.id,
        projectId: row.project_id,
        providerId: row.provider_id,
        title: row.title,
        description: row.description,
        status: row.status,
        plan: row.plan ? JSON.parse(row.plan) : null,
        generatedCode: row.generated_code ? JSON.parse(row.generated_code) : null,
        context: row.context ? JSON.parse(row.context) : null,
        errorMessage: row.error_message,
        startedAt: row.started_at ? new Date(row.started_at) : null,
        completedAt: row.completed_at ? new Date(row.completed_at) : null,
        createdAt: new Date(row.created_at),
        updatedAt: new Date(row.updated_at),
    };
}
// ============================================
// Repository
// ============================================
exports.MissionsRepository = {
    /**
     * Lista missões com filtros.
     */
    findAll(options = {}) {
        const db = (0, connection_1.getDatabase)();
        const { limit = 100, offset = 0, projectId, status, orderBy = 'createdAt', orderDirection = 'desc', } = options;
        const conditions = [];
        const values = [];
        if (projectId) {
            conditions.push('project_id = ?');
            values.push(projectId);
        }
        if (status) {
            if (Array.isArray(status)) {
                conditions.push(`status IN (${status.map(() => '?').join(', ')})`);
                values.push(...status);
            }
            else {
                conditions.push('status = ?');
                values.push(status);
            }
        }
        const whereClause = conditions.length > 0
            ? `WHERE ${conditions.join(' AND ')}`
            : '';
        const columnMap = {
            title: 'title',
            status: 'status',
            createdAt: 'created_at',
            updatedAt: 'updated_at',
        };
        const orderColumn = columnMap[orderBy] || 'created_at';
        const direction = orderDirection.toUpperCase() === 'ASC' ? 'ASC' : 'DESC';
        values.push(limit, offset);
        const stmt = db.prepare(`
      SELECT * FROM missions
      ${whereClause}
      ORDER BY ${orderColumn} ${direction}
      LIMIT ? OFFSET ?
    `);
        const rows = stmt.all(...values);
        return rows.map(rowToMission);
    },
    /**
     * Lista missões de um projeto.
     */
    findByProject(projectId, limit = 50) {
        return this.findAll({ projectId, limit, orderBy: 'createdAt', orderDirection: 'desc' });
    },
    /**
     * Busca missões ativas (não finalizadas).
     */
    findActive(projectId) {
        const activeStatuses = [
            'created',
            'planning',
            'plan_generated',
            'generating_code',
            'code_ready',
            'applying',
        ];
        return this.findAll({ projectId, status: activeStatuses });
    },
    /**
     * Busca uma missão por ID.
     */
    findById(id) {
        const db = (0, connection_1.getDatabase)();
        const stmt = db.prepare('SELECT * FROM missions WHERE id = ?');
        const row = stmt.get(id);
        return row ? rowToMission(row) : null;
    },
    /**
     * Cria uma nova missão.
     */
    create(data) {
        const db = (0, connection_1.getDatabase)();
        const id = (0, connection_1.generateId)();
        const stmt = db.prepare(`
      INSERT INTO missions (id, project_id, provider_id, title, description, status)
      VALUES (?, ?, ?, ?, ?, 'created')
    `);
        stmt.run(id, data.projectId, data.providerId || null, data.title, data.description);
        return this.findById(id);
    },
    /**
     * Atualiza uma missão existente.
     */
    update(id, data) {
        const db = (0, connection_1.getDatabase)();
        const existing = this.findById(id);
        if (!existing) {
            return null;
        }
        const updates = [];
        const values = [];
        if (data.title !== undefined) {
            updates.push('title = ?');
            values.push(data.title);
        }
        if (data.description !== undefined) {
            updates.push('description = ?');
            values.push(data.description);
        }
        if (data.providerId !== undefined) {
            updates.push('provider_id = ?');
            values.push(data.providerId);
        }
        if (data.status !== undefined) {
            updates.push('status = ?');
            values.push(data.status);
        }
        if (data.plan !== undefined) {
            updates.push('plan = ?');
            values.push(JSON.stringify(data.plan));
        }
        if (data.generatedCode !== undefined) {
            updates.push('generated_code = ?');
            values.push(JSON.stringify(data.generatedCode));
        }
        if (data.context !== undefined) {
            updates.push('context = ?');
            values.push(JSON.stringify(data.context));
        }
        if (data.errorMessage !== undefined) {
            updates.push('error_message = ?');
            values.push(data.errorMessage);
        }
        if (data.startedAt !== undefined) {
            updates.push('started_at = ?');
            values.push(data.startedAt ? data.startedAt.toISOString() : null);
        }
        if (data.completedAt !== undefined) {
            updates.push('completed_at = ?');
            values.push(data.completedAt ? data.completedAt.toISOString() : null);
        }
        if (updates.length === 0) {
            return existing;
        }
        values.push(id);
        const stmt = db.prepare(`
      UPDATE missions
      SET ${updates.join(', ')}
      WHERE id = ?
    `);
        stmt.run(...values);
        return this.findById(id);
    },
    /**
     * Atualiza o status de uma missão.
     */
    updateStatus(id, status, errorMessage) {
        const updates = { status };
        if (status === 'planning' || status === 'generating_code') {
            updates.startedAt = new Date();
        }
        if (status === 'completed' || status === 'failed' || status === 'cancelled') {
            updates.completedAt = new Date();
        }
        if (errorMessage) {
            updates.errorMessage = errorMessage;
        }
        return this.update(id, updates);
    },
    /**
     * Salva o plano gerado para uma missão.
     */
    savePlan(id, plan) {
        return this.update(id, {
            plan,
            status: 'plan_generated',
        });
    },
    /**
     * Salva o código gerado para uma missão.
     */
    saveGeneratedCode(id, generatedCode) {
        return this.update(id, {
            generatedCode,
            status: 'code_ready',
        });
    },
    /**
     * Salva o contexto usado na missão.
     */
    saveContext(id, context) {
        return this.update(id, { context });
    },
    /**
     * Remove uma missão (e seus logs em cascata).
     */
    delete(id) {
        const db = (0, connection_1.getDatabase)();
        const stmt = db.prepare('DELETE FROM missions WHERE id = ?');
        const result = stmt.run(id);
        return result.changes > 0;
    },
    /**
     * Conta missões por projeto.
     */
    countByProject(projectId) {
        const db = (0, connection_1.getDatabase)();
        const stmt = db.prepare('SELECT COUNT(*) as count FROM missions WHERE project_id = ?');
        const row = stmt.get(projectId);
        return row.count;
    },
    /**
     * Conta missões por status.
     */
    countByStatus(status, projectId) {
        const db = (0, connection_1.getDatabase)();
        if (projectId) {
            const stmt = db.prepare('SELECT COUNT(*) as count FROM missions WHERE status = ? AND project_id = ?');
            const row = stmt.get(status, projectId);
            return row.count;
        }
        const stmt = db.prepare('SELECT COUNT(*) as count FROM missions WHERE status = ?');
        const row = stmt.get(status);
        return row.count;
    },
    /**
     * Obtém estatísticas das missões de um projeto.
     */
    getProjectStats(projectId) {
        const db = (0, connection_1.getDatabase)();
        const stmt = db.prepare(`
      SELECT status, COUNT(*) as count
      FROM missions
      WHERE project_id = ?
      GROUP BY status
    `);
        const rows = stmt.all(projectId);
        const stats = {
            created: 0,
            planning: 0,
            plan_generated: 0,
            generating_code: 0,
            code_ready: 0,
            applying: 0,
            completed: 0,
            failed: 0,
            cancelled: 0,
        };
        for (const row of rows) {
            stats[row.status] = row.count;
        }
        return stats;
    },
};
//# sourceMappingURL=missions.js.map