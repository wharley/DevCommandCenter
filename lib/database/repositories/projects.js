"use strict";
// Dev Command Center - Projects Repository
// Camada de acesso aos dados de projetos
Object.defineProperty(exports, "__esModule", { value: true });
exports.ProjectsRepository = void 0;
const connection_1 = require("../connection");
function rowToProject(row) {
    return {
        id: row.id,
        name: row.name,
        path: row.path,
        description: row.description,
        defaultProviderId: row.default_provider_id,
        gitRemoteUrl: row.git_remote_url,
        lastOpenedAt: row.last_opened_at ? new Date(row.last_opened_at) : null,
        createdAt: new Date(row.created_at),
        updatedAt: new Date(row.updated_at),
    };
}
// ============================================
// Repository
// ============================================
exports.ProjectsRepository = {
    /**
     * Lista todos os projetos.
     */
    findAll(options = {}) {
        const db = (0, connection_1.getDatabase)();
        const { limit = 100, offset = 0, orderBy = 'lastOpenedAt', orderDirection = 'desc', } = options;
        const columnMap = {
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
        const rows = stmt.all(limit, offset);
        return rows.map(rowToProject);
    },
    /**
     * Busca projetos recentes (últimos abertos).
     */
    findRecent(limit = 10) {
        const db = (0, connection_1.getDatabase)();
        const stmt = db.prepare(`
      SELECT * FROM projects
      WHERE last_opened_at IS NOT NULL
      ORDER BY last_opened_at DESC
      LIMIT ?
    `);
        const rows = stmt.all(limit);
        return rows.map(rowToProject);
    },
    /**
     * Busca um projeto por ID.
     */
    findById(id) {
        const db = (0, connection_1.getDatabase)();
        const stmt = db.prepare('SELECT * FROM projects WHERE id = ?');
        const row = stmt.get(id);
        return row ? rowToProject(row) : null;
    },
    /**
     * Busca um projeto pelo caminho.
     */
    findByPath(path) {
        const db = (0, connection_1.getDatabase)();
        const stmt = db.prepare('SELECT * FROM projects WHERE path = ?');
        const row = stmt.get(path);
        return row ? rowToProject(row) : null;
    },
    /**
     * Cria um novo projeto.
     */
    create(data) {
        const db = (0, connection_1.getDatabase)();
        const id = (0, connection_1.generateId)();
        const stmt = db.prepare(`
      INSERT INTO projects (id, name, path, description, default_provider_id, git_remote_url)
      VALUES (?, ?, ?, ?, ?, ?)
    `);
        stmt.run(id, data.name, data.path, data.description || null, data.defaultProviderId || null, data.gitRemoteUrl || null);
        return this.findById(id);
    },
    /**
     * Atualiza um projeto existente.
     */
    update(id, data) {
        const db = (0, connection_1.getDatabase)();
        const existing = this.findById(id);
        if (!existing) {
            return null;
        }
        const updates = [];
        const values = [];
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
    updateLastOpened(id) {
        const db = (0, connection_1.getDatabase)();
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
    delete(id) {
        const db = (0, connection_1.getDatabase)();
        const stmt = db.prepare('DELETE FROM projects WHERE id = ?');
        const result = stmt.run(id);
        return result.changes > 0;
    },
    /**
     * Conta o total de projetos.
     */
    count() {
        const db = (0, connection_1.getDatabase)();
        const stmt = db.prepare('SELECT COUNT(*) as count FROM projects');
        const row = stmt.get();
        return row.count;
    },
    /**
     * Verifica se um projeto com o caminho já existe.
     */
    existsByPath(path) {
        const db = (0, connection_1.getDatabase)();
        const stmt = db.prepare('SELECT 1 FROM projects WHERE path = ? LIMIT 1');
        const row = stmt.get(path);
        return row !== undefined;
    },
    /**
     * Busca projetos por texto (nome ou descrição).
     */
    search(query, limit = 20) {
        const db = (0, connection_1.getDatabase)();
        const searchTerm = `%${query}%`;
        const stmt = db.prepare(`
      SELECT * FROM projects
      WHERE name LIKE ? OR description LIKE ? OR path LIKE ?
      ORDER BY last_opened_at DESC NULLS LAST
      LIMIT ?
    `);
        const rows = stmt.all(searchTerm, searchTerm, searchTerm, limit);
        return rows.map(rowToProject);
    },
};
//# sourceMappingURL=projects.js.map