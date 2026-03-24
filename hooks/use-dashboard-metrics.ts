"use client";

import { useMemo } from "react";
import type { Comb, CombStatus } from "@/lib/database/types";

export type DashboardPeriodDays = 7 | 30;

type StatusCount<T extends string> = {
  status: T;
  label: string;
  count: number;
};

export interface DashboardMetrics {
  workspacesCreated: number;
  workspacesApplied: number;
  avgLeadTimeMinutes: number | null;
  combStatusCounts: Array<StatusCount<CombStatus>>;
}

const combStatusLabel: Record<CombStatus, string> = {
  active: "Ativo",
  ready_for_review: "Em Revisão",
  applied: "Aplicado",
  discarded: "Descartado",
  archived: "Arquivado",
  error: "Erro",
};

function getPeriodStart(days: DashboardPeriodDays): Date {
  const now = new Date();
  return new Date(now.getTime() - days * 24 * 60 * 60 * 1000);
}

function asDate(value: unknown): Date | null {
  if (value instanceof Date) return value;
  if (typeof value === "string") {
    const parsed = new Date(value);
    return Number.isNaN(parsed.getTime()) ? null : parsed;
  }
  return null;
}

function countCombStatus(combs: Comb[]) {
  const counts = new Map<CombStatus, number>();
  for (const comb of combs) {
    counts.set(comb.status, (counts.get(comb.status) ?? 0) + 1);
  }

  return (Object.keys(combStatusLabel) as CombStatus[]).map((status) => ({
    status,
    label: combStatusLabel[status],
    count: counts.get(status) ?? 0,
  }));
}

export function useDashboardMetrics(
  combs: Comb[],
  periodDays: DashboardPeriodDays,
): DashboardMetrics {
  return useMemo(() => {
    const periodStart = getPeriodStart(periodDays);

    const combsInPeriod = combs.filter((c) => {
      const createdAt = asDate(c.createdAt);
      return !!createdAt && createdAt >= periodStart;
    });
    const appliedInPeriod = combsInPeriod.filter((c) => c.status === "applied");

    return {
      workspacesCreated: combsInPeriod.length,
      workspacesApplied: appliedInPeriod.length,
      avgLeadTimeMinutes: null,
      combStatusCounts: countCombStatus(combs),
    };
  }, [combs, periodDays]);
}
