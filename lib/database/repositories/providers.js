"use strict";
// Dev Command Center - Providers Repository
// Camada de acesso aos dados de provedores de IA
Object.defineProperty(exports, "__esModule", { value: true });
exports.ProvidersRepository = void 0;
const connection_1 = require("../connection");
function rowToProvider(row) {
    return {
        id: row.id,
        name: row.name,
        type: row.type,
        apiKey: row.api_key,
        cliPath: row.cli_path,
        config: row.config ? JSON.parse(row.config) : null,
        isActive: row.is_active === 1,
        createdAt: new Date(row.created_at),
        updatedAt: new Date(row.updated_at),
    };
}
// ============================================
// Repository
// ============================================
exports.ProvidersRepository = {
    /**
     * Lista todos os provedores.
     */
    findAll(options = {}) {
        const db = (0, connection_1.getDatabase)();
        const { limit = 100, offset = 0 } = options;
        const stmt = db.prepare(`
      SELECT * FROM providers
      ORDER BY name ASC
      LIMIT ? OFFSET ?
    `);
        const rows = stmt.all(limit, offset);
        return rows.map(rowToProvider);
    },
    /**
     * Lista apenas provedores ativos.
     */
    findActive() {
        const db = (0, connection_1.getDatabase)();
        const stmt = db.prepare(`
      SELECT * FROM providers
      WHERE is_active = 1
      ORDER BY name ASC
    `);
        const rows = stmt.all();
        return rows.map(rowToProvider);
    },
    /**
     * Busca um provedor por ID.
     */
    findById(id) {
        const db = (0, connection_1.getDatabase)();
        const stmt = db.prepare('SELECT * FROM providers WHERE id = ?');
        const row = stmt.get(id);
        return row ? rowToProvider(row) : null;
    },
    /**
     * Busca provedores por tipo.
     */
    findByType(type) {
        const db = (0, connection_1.getDatabase)();
        const stmt = db.prepare(`
      SELECT * FROM providers
      WHERE type = ?
      ORDER BY name ASC
    `);
        const rows = stmt.all(type);
        return rows.map(rowToProvider);
    },
    /**
     * Cria um novo provedor.
     */
    create(data) {
        const db = (0, connection_1.getDatabase)();
        const id = (0, connection_1.generateId)();
        const stmt = db.prepare(`
      INSERT INTO providers (id, name, type, api_key, cli_path, config, is_active)
      VALUES (?, ?, ?, ?, ?, ?, ?)
    `);
        stmt.run(id, data.name, data.type, data.apiKey || null, data.cliPath || null, data.config ? JSON.stringify(data.config) : null, data.isActive !== false ? 1 : 0);
        return this.findById(id);
    },
    /**
     * Atualiza um provedor existente.
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
        if (data.type !== undefined) {
            updates.push('type = ?');
            values.push(data.type);
        }
        if (data.apiKey !== undefined) {
            updates.push('api_key = ?');
            values.push(data.apiKey);
        }
        if (data.cliPath !== undefined) {
            updates.push('cli_path = ?');
            values.push(data.cliPath);
        }
        if (data.config !== undefined) {
            updates.push('config = ?');
            values.push(JSON.stringify(data.config));
        }
        if (data.isActive !== undefined) {
            updates.push('is_active = ?');
            values.push(data.isActive ? 1 : 0);
        }
        if (updates.length === 0) {
            return existing;
        }
        values.push(id);
        const stmt = db.prepare(`
      UPDATE providers
      SET ${updates.join(', ')}
      WHERE id = ?
    `);
        stmt.run(...values);
        return this.findById(id);
    },
    /**
     * Remove um provedor.
     */
    delete(id) {
        const db = (0, connection_1.getDatabase)();
        const stmt = db.prepare('DELETE FROM providers WHERE id = ?');
        const result = stmt.run(id);
        return result.changes > 0;
    },
    /**
     * Conta o total de provedores.
     */
    count() {
        const db = (0, connection_1.getDatabase)();
        const stmt = db.prepare('SELECT COUNT(*) as count FROM providers');
        const row = stmt.get();
        return row.count;
    },
    /**
     * Verifica se um provedor com o nome já existe.
     */
    existsByName(name) {
        const db = (0, connection_1.getDatabase)();
        const stmt = db.prepare('SELECT 1 FROM providers WHERE name = ? LIMIT 1');
        const row = stmt.get(name);
        return row !== undefined;
    },
};
//# sourceMappingURL=providers.js.map