"use client";

import { useMemo } from "react";
import type { Comb, CombStatus, Mission, MissionStatus } from "@/lib/database/types";

export type DashboardPeriodDays = 7 | 30;

type StatusCount<T extends string> = {
  status: T;
  label: string;
  count: number;
};

export interface DashboardMetrics {
  missionsCreated: number;
  missionsCompleted: number;
  avgLeadTimeMinutes: number | null;
  missionStatusCounts: Array<StatusCount<MissionStatus>>;
  combStatusCounts: Array<StatusCount<CombStatus>>;
}

const missionStatusLabel: Record<MissionStatus, string> = {
  created: "Criada",
  planning: "Planejamento",
  plan_generated: "Plano pronto",
  generating_code: "Gerando código",
  code_ready: "Código pronto",
  applying: "Aplicando",
  completed: "Concluída",
  failed: "Falhou",
  cancelled: "Cancelada",
};

const combStatusLabel: Record<CombStatus, string> = {
  active: "Ativa",
  ready_for_review: "Revisão",
  applied: "Aplicada",
  discarded: "Descartada",
  archived: "Arquivada",
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

function countMissionStatus(missions: Mission[]) {
  const counts = new Map<MissionStatus, number>();
  for (const mission of missions) {
    counts.set(mission.status, (counts.get(mission.status) ?? 0) + 1);
  }

  return (Object.keys(missionStatusLabel) as MissionStatus[]).map((status) => ({
    status,
    label: missionStatusLabel[status],
    count: counts.get(status) ?? 0,
  }));
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
  missions: Mission[],
  combs: Comb[],
  periodDays: DashboardPeriodDays,
): DashboardMetrics {
  return useMemo(() => {
    const periodStart = getPeriodStart(periodDays);

    const missionsInPeriod = missions.filter((m) => {
      const createdAt = asDate(m.createdAt);
      return !!createdAt && createdAt >= periodStart;
    });
    const completedInPeriod = missionsInPeriod.filter((m) => m.status === "completed");

    const leadTimes = completedInPeriod
      .map((mission) => {
        const completedAt = asDate(mission.completedAt);
        const createdAt = asDate(mission.createdAt);
        if (!completedAt || !createdAt) return null;
        const diffMs = completedAt.getTime() - createdAt.getTime();
        return diffMs > 0 ? diffMs / (1000 * 60) : null;
      })
      .filter((value): value is number => value !== null);

    const avgLeadTimeMinutes =
      leadTimes.length > 0
        ? Math.round(
            (leadTimes.reduce((sum, value) => sum + value, 0) / leadTimes.length) * 100,
          ) / 100
        : null;

    return {
      missionsCreated: missionsInPeriod.length,
      missionsCompleted: completedInPeriod.length,
      avgLeadTimeMinutes,
      missionStatusCounts: countMissionStatus(missionsInPeriod),
      combStatusCounts: countCombStatus(combs),
    };
  }, [missions, combs, periodDays]);
}
