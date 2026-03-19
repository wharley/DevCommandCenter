// Dev Command Center - Missions Repository
// Camada de acesso aos dados de missões de código

import { getDatabase, generateId } from '../connection';
import type {
  Mission,
  MissionStatus,
  MissionPlan,
  GeneratedCode,
  MissionContext,
  PendingCommand,
  CreateMissionDTO,
  UpdateMissionDTO,
  MissionsQueryOptions,
  WallStatus,
  LastGitSummary,
} from '../types';

// ============================================
// Helpers de conversão
// ============================================

interface MissionRow {
  id: string;
  project_id: string;
  provider_id: string | null;
  plan_provider_id?: string | null;
  code_provider_id?: string | null;
  title: string;
  description: string;
  status: string;
  mission_type?: string | null;
  plan: string | null;
  generated_code: string | null;
  context: string | null;
  preserve_instructions?: string | null;
  error_message: string | null;
  code_generation_attempts: number | null;
  is_committed: number | null;
  is_pushed: number | null;
  pending_commands: string | null;
  worktree_path?: string | null;
  worktree_branch?: string | null;
  base_branch?: string | null;
  target_branch?: string | null;
  last_output_summary?: string | null;
  last_git_summary?: string | null;
  wall_status?: string | null;
  started_at: string | null;
  completed_at: string | null;
  created_at: string;
  updated_at: string;
}

function rowToMission(row: MissionRow): Mission {
  const missionType =
    row.mission_type === 'agents_cli'
      ? 'agents_cli'
      : row.mission_type === 'analysis'
        ? 'analysis'
        : 'implementation';
  return {
    id: row.id,
    projectId: row.project_id,
    providerId: row.provider_id,
    planProviderId: row.plan_provider_id ?? null,
    codeProviderId: row.code_provider_id ?? null,
    title: row.title,
    description: row.description,
    status: row.status as MissionStatus,
    missionType,
    plan: row.plan ? JSON.parse(row.plan) : null,
    generatedCode: row.generated_code ? JSON.parse(row.generated_code) : null,
    context: row.context ? JSON.parse(row.context) : null,
    preserveInstructions: row.preserve_instructions ?? null,
    errorMessage: row.error_message,
    codeGenerationAttempts: row.code_generation_attempts ?? 0,
    isCommitted: row.is_committed ? Boolean(row.is_committed) : false,
    isPushed: row.is_pushed ? Boolean(row.is_pushed) : false,
    pendingCommands: row.pending_commands ? JSON.parse(row.pending_commands) : null,
    worktreePath: row.worktree_path ?? null,
    worktreeBranch: row.worktree_branch ?? null,
    baseBranch: row.base_branch ?? null,
    targetBranch: row.target_branch ?? null,
    lastOutputSummary: row.last_output_summary ?? null,
    lastGitSummary: row.last_git_summary
      ? (JSON.parse(row.last_git_summary) as LastGitSummary)
      : null,
    wallStatus: (row.wall_status as WallStatus | undefined) ?? null,
    startedAt: row.started_at ? new Date(row.started_at) : null,
    completedAt: row.completed_at ? new Date(row.completed_at) : null,
    createdAt: new Date(row.created_at),
    updatedAt: new Date(row.updated_at),
  };
}

// ============================================
// Repository
// ============================================

export const MissionsRepository = {
  /**
   * Lista missões com filtros.
   */
  findAll(options: MissionsQueryOptions = {}): Mission[] {
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
    
    const rows = stmt.all(...values) as MissionRow[];
    return rows.map(rowToMission);
  },

  /**
   * Lista missões de um projeto.
   */
  findByProject(projectId: string, limit: number = 50): Mission[] {
    return this.findAll({ projectId, limit, orderBy: 'createdAt', orderDirection: 'desc' });
  },

  /**
   * Busca missões ativas (não finalizadas).
   */
  findActive(projectId?: string): Mission[] {
    const activeStatuses: MissionStatus[] = [
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
   * Busca missões em execução no mesmo projeto (planning, generating, applying).
   * Usado para garantir apenas uma missão ativa por projeto (evita conflitos Git).
   * @param projectId ID do projeto
   * @param excludeMissionId ID da missão atual (excluída da busca)
   */
  findInProgress(
    projectId: string,
    excludeMissionId?: string,
  ): Mission[] {
    const inProgressStatuses: MissionStatus[] = [
      'planning',
      'plan_generated',
      'generating_code',
      'code_ready',
      'applying',
    ];
    const options: { projectId: string; status: MissionStatus[] } = {
      projectId,
      status: inProgressStatuses,
    };
    const missions = this.findAll({ ...options, limit: 50, offset: 0 });
    if (excludeMissionId) {
      return missions.filter((m) => m.id !== excludeMissionId);
    }
    return missions;
  },

  /**
   * Busca missões que modificam ou dependem do working tree Git no mesmo projeto.
   * Usado para serializar generateCode e applyChanges (evita conflitos de git apply).
   * Não inclui planning/plan_generated pois essas etapas só leem do repositório.
   * @param projectId ID do projeto
   * @param excludeMissionId ID da missão atual (excluída da busca)
   */
  findModifyingGit(
    projectId: string,
    excludeMissionId?: string,
  ): Mission[] {
    const modifyingStatuses: MissionStatus[] = [
      'generating_code',
      'code_ready',
      'applying',
    ];
    const options: { projectId: string; status: MissionStatus[] } = {
      projectId,
      status: modifyingStatuses,
    };
    const missions = this.findAll({ ...options, limit: 50, offset: 0 });
    if (excludeMissionId) {
      return missions.filter((m) => m.id !== excludeMissionId);
    }
    return missions;
  },

  /**
   * Busca uma missão por ID.
   */
  findById(id: string): Mission | null {
    const db = getDatabase();
    
    const stmt = db.prepare('SELECT * FROM missions WHERE id = ?');
    const row = stmt.get(id) as MissionRow | undefined;
    
    return row ? rowToMission(row) : null;
  },

  /**
   * Cria uma nova missão.
   */
  create(data: CreateMissionDTO): Mission {
    const db = getDatabase();
    const id = generateId();

    const missionType = data.missionType ?? 'implementation';
    const isAgentsCli = missionType === 'agents_cli';
    const wallStatus = isAgentsCli ? 'running' : null;
    const stmt = db.prepare(`
      INSERT INTO missions (id, project_id, provider_id, plan_provider_id, code_provider_id, title, description, preserve_instructions, mission_type, status, base_branch, wall_status)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    `);

    stmt.run(
      id,
      data.projectId,
      data.providerId || null,
      data.planProviderId ?? null,
      data.codeProviderId ?? null,
      data.title,
      data.description,
      data.preserveInstructions ?? null,
      missionType,
      'created',
      data.baseBranch ?? null,
      wallStatus
    );

    return this.findById(id)!;
  },

  /**
   * Atualiza uma missão existente.
   */
  update(id: string, data: UpdateMissionDTO): Mission | null {
    const db = getDatabase();
    const existing = this.findById(id);
    
    if (!existing) {
      return null;
    }
    
    const updates: string[] = [];
    const values: (string | null)[] = [];
    
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
    if (data.planProviderId !== undefined) {
      updates.push('plan_provider_id = ?');
      values.push(data.planProviderId);
    }
    if (data.codeProviderId !== undefined) {
      updates.push('code_provider_id = ?');
      values.push(data.codeProviderId);
    }
    if (data.missionType !== undefined) {
      updates.push('mission_type = ?');
      values.push(data.missionType);
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
    if (data.preserveInstructions !== undefined) {
      updates.push('preserve_instructions = ?');
      values.push(data.preserveInstructions);
    }
    if (data.errorMessage !== undefined) {
      updates.push('error_message = ?');
      values.push(data.errorMessage);
    }
    if (data.codeGenerationAttempts !== undefined) {
      updates.push('code_generation_attempts = ?');
      values.push(String(data.codeGenerationAttempts));
    }
    if (data.isCommitted !== undefined) {
      updates.push('is_committed = ?');
      values.push(data.isCommitted ? '1' : '0');
    }
    if (data.isPushed !== undefined) {
      updates.push('is_pushed = ?');
      values.push(data.isPushed ? '1' : '0');
    }
    if (data.pendingCommands !== undefined) {
      updates.push('pending_commands = ?');
      values.push(data.pendingCommands ? JSON.stringify(data.pendingCommands) : null);
    }
    if (data.startedAt !== undefined) {
      updates.push('started_at = ?');
      values.push(data.startedAt ? data.startedAt.toISOString() : null);
    }
    if (data.completedAt !== undefined) {
      updates.push('completed_at = ?');
      values.push(data.completedAt ? data.completedAt.toISOString() : null);
    }
    if (data.worktreePath !== undefined) {
      updates.push('worktree_path = ?');
      values.push(data.worktreePath);
    }
    if (data.worktreeBranch !== undefined) {
      updates.push('worktree_branch = ?');
      values.push(data.worktreeBranch);
    }
    if (data.baseBranch !== undefined) {
      updates.push('base_branch = ?');
      values.push(data.baseBranch);
    }
    if (data.targetBranch !== undefined) {
      updates.push('target_branch = ?');
      values.push(data.targetBranch);
    }
    if (data.lastOutputSummary !== undefined) {
      updates.push('last_output_summary = ?');
      values.push(data.lastOutputSummary);
    }
    if (data.lastGitSummary !== undefined) {
      updates.push('last_git_summary = ?');
      values.push(data.lastGitSummary ? JSON.stringify(data.lastGitSummary) : null);
    }
    if (data.wallStatus !== undefined) {
      updates.push('wall_status = ?');
      values.push(data.wallStatus);
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
  updateStatus(id: string, status: MissionStatus, errorMessage?: string): Mission | null {
    const updates: UpdateMissionDTO = { status };
    
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
  savePlan(id: string, plan: MissionPlan): Mission | null {
    return this.update(id, {
      plan,
      status: 'plan_generated',
    });
  },

  /**
   * Salva o código gerado para uma missão.
   */
  saveGeneratedCode(id: string, generatedCode: GeneratedCode): Mission | null {
    return this.update(id, {
      generatedCode,
      status: 'code_ready',
    });
  },

  /**
   * Salva o contexto usado na missão.
   */
  saveContext(id: string, context: MissionContext): Mission | null {
    return this.update(id, { context });
  },

  /**
   * Remove uma missão (e seus logs em cascata).
   */
  delete(id: string): boolean {
    const db = getDatabase();
    
    const stmt = db.prepare('DELETE FROM missions WHERE id = ?');
    const result = stmt.run(id);
    
    return result.changes > 0;
  },

  /**
   * Conta missões por projeto.
   */
  countByProject(projectId: string): number {
    const db = getDatabase();
    
    const stmt = db.prepare('SELECT COUNT(*) as count FROM missions WHERE project_id = ?');
    const row = stmt.get(projectId) as { count: number };
    
    return row.count;
  },

  /**
   * Conta missões por status.
   */
  countByStatus(status: MissionStatus, projectId?: string): number {
    const db = getDatabase();
    
    if (projectId) {
      const stmt = db.prepare('SELECT COUNT(*) as count FROM missions WHERE status = ? AND project_id = ?');
      const row = stmt.get(status, projectId) as { count: number };
      return row.count;
    }
    
    const stmt = db.prepare('SELECT COUNT(*) as count FROM missions WHERE status = ?');
    const row = stmt.get(status) as { count: number };
    return row.count;
  },

  /**
   * Obtém estatísticas das missões de um projeto.
   */
  getProjectStats(projectId: string): Record<MissionStatus, number> {
    const db = getDatabase();
    
    const stmt = db.prepare(`
      SELECT status, COUNT(*) as count
      FROM missions
      WHERE project_id = ?
      GROUP BY status
    `);
    
    const rows = stmt.all(projectId) as { status: string; count: number }[];
    
    const stats: Record<string, number> = {
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
    
    return stats as Record<MissionStatus, number>;
  },

  /**
   * Busca missões por status.
   */
  findByStatus(status: MissionStatus, projectId?: string): Mission[] {
    return this.findAll({ status, projectId });
  },

  /**
   * Busca missões por texto.
   */
  search(query: string, projectId?: string, limit: number = 20): Mission[] {
    const db = getDatabase();
    const searchTerm = `%${query}%`;
    
    let sql = `
      SELECT * FROM missions
      WHERE (title LIKE ? OR description LIKE ?)
    `;
    const values: (string | number)[] = [searchTerm, searchTerm];
    
    if (projectId) {
      sql += ' AND project_id = ?';
      values.push(projectId);
    }
    
    sql += ' ORDER BY created_at DESC LIMIT ?';
    values.push(limit);
    
    const stmt = db.prepare(sql);
    const rows = stmt.all(...values) as MissionRow[];
    return rows.map(rowToMission);
  },

  /**
   * Atualiza o plano de uma missão.
   */
  updatePlan(id: string, plan: MissionPlan): Mission | null {
    return this.savePlan(id, plan);
  },

  /**
   * Atualiza o código gerado de uma missão.
   */
  updateGeneratedCode(id: string, generatedCode: GeneratedCode): Mission | null {
    return this.saveGeneratedCode(id, generatedCode);
  },

  /**
   * Inicia uma missão (muda status para 'planning').
   */
  start(id: string): Mission | null {
    return this.updateStatus(id, 'planning');
  },

  /**
   * Completa uma missão.
   */
  complete(id: string, summary?: string): Mission | null {
    const updates: UpdateMissionDTO = {
      status: 'completed',
      completedAt: new Date(),
    };
    
    if (summary) {
      // Podemos armazenar o summary no errorMessage ou em outro campo
      // Por ora, vamos usar o context
      const existing = this.findById(id);
      if (existing) {
        updates.context = {
          ...(existing.context || {}),
          completionSummary: summary,
        } as MissionContext;
      }
    }
    
    return this.update(id, updates);
  },

  /**
   * Marca uma missão como falha.
   */
  fail(id: string, error: string): Mission | null {
    return this.updateStatus(id, 'failed', error);
  },

  /**
   * Cancela uma missão.
   */
  cancel(id: string): Mission | null {
    const mission = this.updateStatus(id, 'cancelled');
    if (mission?.missionType === 'agents_cli') {
      return this.update(id, { wallStatus: 'canceled' }) ?? mission;
    }
    return mission;
  },

  /**
   * Obtém uma missão com todos os detalhes (incluindo logs).
   */
  getFullMission(id: string): { mission: Mission; logs: any[] } | null {
    const mission = this.findById(id);
    
    if (!mission) {
      return null;
    }
    
    // Importar os logs da missão
    const db = getDatabase();
    const stmt = db.prepare(`
      SELECT * FROM mission_logs
      WHERE mission_id = ?
      ORDER BY created_at ASC
    `);
    
    const logs = stmt.all(id);
    
    return { mission, logs };
  },
};
