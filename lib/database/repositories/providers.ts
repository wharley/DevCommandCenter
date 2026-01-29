// Dev Command Center - Providers Repository
// Camada de acesso aos dados de provedores de IA

import { getDatabase, generateId } from '../connection';
import type {
  Provider,
  ProviderConfig,
  CreateProviderDTO,
  UpdateProviderDTO,
  PaginationOptions,
} from '../types';

// ============================================
// Helpers de conversão
// ============================================

interface ProviderRow {
  id: string;
  name: string;
  type: string;
  api_key: string | null;
  cli_path: string | null;
  config: string | null;
  is_active: number;
  created_at: string;
  updated_at: string;
}

function rowToProvider(row: ProviderRow): Provider {
  return {
    id: row.id,
    name: row.name,
    type: row.type as Provider['type'],
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

export const ProvidersRepository = {
  /**
   * Lista todos os provedores.
   */
  findAll(options: PaginationOptions = {}): Provider[] {
    const db = getDatabase();
    const { limit = 100, offset = 0 } = options;
    
    const stmt = db.prepare(`
      SELECT * FROM providers
      ORDER BY name ASC
      LIMIT ? OFFSET ?
    `);
    
    const rows = stmt.all(limit, offset) as ProviderRow[];
    return rows.map(rowToProvider);
  },

  /**
   * Lista apenas provedores ativos.
   */
  findActive(): Provider[] {
    const db = getDatabase();
    
    const stmt = db.prepare(`
      SELECT * FROM providers
      WHERE is_active = 1
      ORDER BY name ASC
    `);
    
    const rows = stmt.all() as ProviderRow[];
    return rows.map(rowToProvider);
  },

  /**
   * Busca um provedor por ID.
   */
  findById(id: string): Provider | null {
    const db = getDatabase();
    
    const stmt = db.prepare('SELECT * FROM providers WHERE id = ?');
    const row = stmt.get(id) as ProviderRow | undefined;
    
    return row ? rowToProvider(row) : null;
  },

  /**
   * Busca provedores por tipo.
   */
  findByType(type: Provider['type']): Provider[] {
    const db = getDatabase();
    
    const stmt = db.prepare(`
      SELECT * FROM providers
      WHERE type = ?
      ORDER BY name ASC
    `);
    
    const rows = stmt.all(type) as ProviderRow[];
    return rows.map(rowToProvider);
  },

  /**
   * Cria um novo provedor.
   */
  create(data: CreateProviderDTO): Provider {
    const db = getDatabase();
    const id = generateId();
    
    const stmt = db.prepare(`
      INSERT INTO providers (id, name, type, api_key, cli_path, config, is_active)
      VALUES (?, ?, ?, ?, ?, ?, ?)
    `);
    
    stmt.run(
      id,
      data.name,
      data.type,
      data.apiKey || null,
      data.cliPath || null,
      data.config ? JSON.stringify(data.config) : null,
      data.isActive !== false ? 1 : 0
    );
    
    return this.findById(id)!;
  },

  /**
   * Atualiza um provedor existente.
   */
  update(id: string, data: UpdateProviderDTO): Provider | null {
    const db = getDatabase();
    const existing = this.findById(id);
    
    if (!existing) {
      return null;
    }
    
    const updates: string[] = [];
    const values: (string | number | null)[] = [];
    
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
  delete(id: string): boolean {
    const db = getDatabase();
    
    const stmt = db.prepare('DELETE FROM providers WHERE id = ?');
    const result = stmt.run(id);
    
    return result.changes > 0;
  },

  /**
   * Conta o total de provedores.
   */
  count(): number {
    const db = getDatabase();
    
    const stmt = db.prepare('SELECT COUNT(*) as count FROM providers');
    const row = stmt.get() as { count: number };
    
    return row.count;
  },

  /**
   * Verifica se um provedor com o nome já existe.
   */
  existsByName(name: string): boolean {
    const db = getDatabase();
    
    const stmt = db.prepare('SELECT 1 FROM providers WHERE name = ? LIMIT 1');
    const row = stmt.get(name);
    
    return row !== undefined;
  },

  /**
   * Ativa ou desativa um provedor.
   */
  setActive(id: string, isActive: boolean): Provider | null {
    return this.update(id, { isActive });
  },

  /**
   * Testa a conexão com um provedor.
   * Retorna true se a conexão for bem-sucedida, false caso contrário.
   */
  async testConnection(id: string): Promise<{ success: boolean; error?: string }> {
    const provider = this.findById(id);
    
    if (!provider) {
      return { success: false, error: 'Provider not found' };
    }
    
    // Para provedores de API, verificamos se a API key está configurada
    if (provider.type === 'openai' || provider.type === 'anthropic' || provider.type === 'google') {
      if (!provider.apiKey) {
        return { success: false, error: 'API key not configured' };
      }
      // TODO: Implementar teste real de conexão com a API
      return { success: true };
    }
    
    // Para provedores CLI, verificamos se o path está configurado
    if (provider.type === 'cursor' || provider.type === 'vscode') {
      if (!provider.cliPath) {
        return { success: false, error: 'CLI path not configured' };
      }
      // TODO: Verificar se o executável existe
      return { success: true };
    }
    
    return { success: true };
  },
};
