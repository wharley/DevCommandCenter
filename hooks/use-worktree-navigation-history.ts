import { useCallback, useEffect, useRef, useState, type Dispatch, type SetStateAction } from "react";
import type { Comb } from "@/lib/database/types";

const MAX_STACK = 50;

type SetState<T> = Dispatch<SetStateAction<T>>;

/**
 * Pilha de navegação entre worktrees (combs), semelhante ao histórico do browser.
 * Grava ao mudar para outro comb; voltar/avançar não empilha.
 */
export function useWorktreeNavigationHistory(
  combs: Comb[],
  activeCombId: string | null,
  setActiveCombId: SetState<string | null>,
  setSelectedProjectId: SetState<string | null>,
) {
  const backStackRef = useRef<string[]>([]);
  const forwardStackRef = useRef<string[]>([]);
  const activeCombIdRef = useRef<string | null>(activeCombId);
  const combsRef = useRef(combs);
  combsRef.current = combs;
  activeCombIdRef.current = activeCombId;

  const [, setVersion] = useState(0);
  const bump = useCallback(() => setVersion((v) => v + 1), []);

  useEffect(() => {
    const ids = new Set(combs.map((c) => c.id));
    const nb = backStackRef.current.filter((id) => ids.has(id));
    const nf = forwardStackRef.current.filter((id) => ids.has(id));
    if (nb.length !== backStackRef.current.length || nf.length !== forwardStackRef.current.length) {
      backStackRef.current = nb;
      forwardStackRef.current = nf;
      bump();
    }
  }, [combs, bump]);

  const navigateToComb = useCallback(
    (combId: string | null, options?: { recordHistory?: boolean }) => {
      const record = options?.recordHistory !== false;
      const prev = activeCombIdRef.current;
      if (record && prev && combId && prev !== combId) {
        backStackRef.current = [...backStackRef.current, prev].slice(-MAX_STACK);
        forwardStackRef.current = [];
        bump();
      }
      if (combId) {
        const comb = combsRef.current.find((c) => c.id === combId);
        if (comb) setSelectedProjectId(comb.projectId);
      }
      setActiveCombId(combId);
    },
    [bump, setActiveCombId, setSelectedProjectId],
  );

  const goBack = useCallback(() => {
    while (backStackRef.current.length > 0) {
      const stack = [...backStackRef.current];
      const target = stack.pop()!;
      backStackRef.current = stack;
      const comb = combsRef.current.find((c) => c.id === target);
      if (!comb) {
        bump();
        continue;
      }
      const cur = activeCombIdRef.current;
      if (cur) {
        forwardStackRef.current = [...forwardStackRef.current, cur].slice(-MAX_STACK);
      }
      bump();
      setSelectedProjectId(comb.projectId);
      setActiveCombId(target);
      return;
    }
  }, [bump, setActiveCombId, setSelectedProjectId]);

  const goForward = useCallback(() => {
    while (forwardStackRef.current.length > 0) {
      const stack = [...forwardStackRef.current];
      const target = stack.pop()!;
      forwardStackRef.current = stack;
      const comb = combsRef.current.find((c) => c.id === target);
      if (!comb) {
        bump();
        continue;
      }
      const cur = activeCombIdRef.current;
      if (cur) {
        backStackRef.current = [...backStackRef.current, cur].slice(-MAX_STACK);
      }
      bump();
      setSelectedProjectId(comb.projectId);
      setActiveCombId(target);
      return;
    }
  }, [bump, setActiveCombId, setSelectedProjectId]);

  const canGoBack = backStackRef.current.length > 0;
  const canGoForward = forwardStackRef.current.length > 0;

  return {
    navigateToComb,
    goBack,
    goForward,
    canGoBack,
    canGoForward,
  };
}
