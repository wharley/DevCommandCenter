import { useState, useEffect, useCallback } from "react";

interface MergePermissionsResult {
  canMerge: boolean;
  reason?: string | null;
  isProtected: boolean;
  requiresPR: boolean;
  isLocalOnly: boolean;
  warning?: string;
  isLoading: boolean;
  error?: string;
  refetch: () => void;
}

/**
 * Hook para verificar permissões de merge em uma branch
 * Verifica se o usuário tem permissão para fazer push/merge e se a branch está protegida
 */
export function useMergePermissions(
  combId: string | null | undefined,
  targetBranch?: string,
  options?: {
    enabled?: boolean; // Se false, não executa a verificação automaticamente
  }
): MergePermissionsResult {
  const [state, setState] = useState<Omit<MergePermissionsResult, "refetch">>({
    canMerge: true, // Otimista por padrão
    isProtected: false,
    requiresPR: false,
    isLocalOnly: false,
    isLoading: false,
  });

  const checkPermissions = useCallback(async () => {
    if (!combId || !window.desktopAPI?.comb?.checkMergePermissions) {
      setState({
        canMerge: true,
        isProtected: false,
        requiresPR: false,
        isLocalOnly: false,
        isLoading: false,
      });
      return;
    }

    setState((prev) => ({ ...prev, isLoading: true, error: undefined }));

    try {
      const result = await window.desktopAPI.comb.checkMergePermissions(
        combId,
        targetBranch
      );

      setState({
        canMerge: result.canMerge,
        reason: result.reason,
        isProtected: result.isProtected,
        requiresPR: result.requiresPR,
        isLocalOnly: result.isLocalOnly,
        warning: result.warning,
        isLoading: false,
      });
    } catch (err) {
      setState({
        canMerge: true, // Em caso de erro, permitir tentativa
        isProtected: false,
        requiresPR: false,
        isLocalOnly: false,
        isLoading: false,
        error: err instanceof Error ? err.message : "Erro ao verificar permissões",
        warning: "Não foi possível verificar permissões. Prossiga com cautela.",
      });
    }
  }, [combId, targetBranch]);

  useEffect(() => {
    const enabled = options?.enabled !== false;
    if (enabled) {
      checkPermissions();
    }
  }, [checkPermissions, options?.enabled]);

  return {
    ...state,
    refetch: checkPermissions,
  };
}
