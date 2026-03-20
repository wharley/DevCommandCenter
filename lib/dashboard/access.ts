export interface DashboardAccessContext {
  userEmail?: string | null;
  userId?: string | null;
}

export interface DashboardAccessResult {
  enabled: boolean;
  reason?: string;
}

export function getDashboardAccessContext(): DashboardAccessContext {
  // Aberto para validação com testers.
  // Futuramente, preencher com dados reais de sessão/usuário.
  return {
    userEmail: null,
    userId: null,
  };
}

export function canAccessDashboard(
  _context: DashboardAccessContext,
): DashboardAccessResult {
  // Ponto único de decisão para monetização futura por usuário/email.
  return { enabled: true };
}
