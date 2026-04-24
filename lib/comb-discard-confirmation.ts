/**
 * Copy + severity for removing a comb (worktree): used before `comb_discard`.
 * Calls `comb_check_unpushed` when the desktop bridge exposes it.
 */
export type CombDiscardDialogCopy = {
  title: string;
  description: string;
  confirmLabel: string;
  /** Use destructive confirm button when data loss is likely or verification failed */
  confirmVariant: "default" | "destructive";
};

const DEFAULT_COPY: CombDiscardDialogCopy = {
  title: "Remover workspace?",
  description: "O worktree e os panes associados serão removidos.",
  confirmLabel: "Remover",
  confirmVariant: "default",
};

export async function getCombDiscardDialogCopy(combId: string): Promise<CombDiscardDialogCopy> {
  if (typeof window === "undefined" || !window.desktopAPI?.comb?.checkUnpushed) {
    return DEFAULT_COPY;
  }

  try {
    const info = await window.desktopAPI.comb.checkUnpushed(combId);
    if (!info?.hasUnpushed || info.count <= 0) {
      return DEFAULT_COPY;
    }

    const commitPreview = info.commits
      .slice(0, 5)
      .map((c) => `  • ${c}`)
      .join("\n");
    const moreText =
      info.count > 5 ? `\n  ... e mais ${info.count - 5} commit(s)` : "";

    return {
      title: "Commits não enviados",
      description: `Este workspace tem ${info.count} commit(s) não enviado(s) para o remoto:\n\n${commitPreview}${moreText}\n\nRemover elimina o worktree e a branch local; commits que ainda não foram enviados deixam de estar referenciados por esta branch.`,
      confirmLabel: "Remover mesmo assim",
      confirmVariant: "destructive",
    };
  } catch {
    return {
      title: "Não foi possível verificar o remoto",
      description:
        "Não foi possível confirmar se existem commits por enviar. Remover o workspace elimina o worktree e a branch local; pode perder trabalho se ainda não fez push.",
      confirmLabel: "Remover mesmo assim",
      confirmVariant: "destructive",
    };
  }
}
