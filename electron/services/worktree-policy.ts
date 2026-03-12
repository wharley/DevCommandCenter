/**
 * Worktree Policy - Constantes e regras para governança de worktrees DCC
 *
 * Documentação completa: docs/WORKTREE_POLICY.md
 */

/** Prefixo para worktrees criados pelo DCC */
export const WORKTREE_PREFIX = "dcc-";

/** Padrão de nome: dcc-{identificador}-{timestamp} */
export const WORKTREE_NAME_PATTERN = /^dcc-.+-[0-9]{8}-[0-9]{6}$/;

/** Idade máxima em dias antes de worktree ser candidato à limpeza */
export const WORKTREE_MAX_AGE_DAYS = 7;

/** Nome do arquivo de lock no root do worktree */
export const WORKTREE_LOCK_FILE = ".dcc-worktree-lock";

/** @deprecated Legacy local mode. Default storage is now global (outside repo). */
export const WORKTREE_SUBDIR = ".dcc-worktrees";

/**
 * Gera nome padrão para worktree DCC
 * @param identifier missionId ou branch-shortHash
 * @param timestamp opcional, usa Date.now() se omitido
 */
export function buildWorktreeName(
  identifier: string,
  timestamp?: Date,
): string {
  const ts = timestamp ?? new Date();
  const yyyy = ts.getFullYear();
  const mm = String(ts.getMonth() + 1).padStart(2, "0");
  const dd = String(ts.getDate()).padStart(2, "0");
  const hh = String(ts.getHours()).padStart(2, "0");
  const min = String(ts.getMinutes()).padStart(2, "0");
  const ss = String(ts.getSeconds()).padStart(2, "0");
  const tsStr = `${yyyy}${mm}${dd}-${hh}${min}${ss}`;
  const safeId = identifier.replace(/[^a-zA-Z0-9-_]/g, "-").slice(0, 50);
  return `${WORKTREE_PREFIX}${safeId}-${tsStr}`;
}
