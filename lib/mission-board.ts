import type { Mission, MissionStatus } from "./database/types";

/** Kanban column ids aligned to the pipeline. */
export type MissionColumnId =
  | "todo"
  | "plan"
  | "code"
  | "apply"
  | "done";

/** Maps each mission status to a column. */
export const STATUS_TO_COLUMN: Record<MissionStatus, MissionColumnId> = {
  created: "todo",
  planning: "plan",
  plan_generated: "plan",
  generating_code: "code",
  code_ready: "code",
  applying: "apply",
  completed: "done",
  failed: "done",
  cancelled: "done",
};

/** Column display labels and order for the board. */
export const COLUMN_ORDER: MissionColumnId[] = [
  "todo",
  "plan",
  "code",
  "apply",
  "done",
];

export const COLUMN_LABELS: Record<MissionColumnId, string> = {
  todo: "A fazer",
  plan: "Gerar plano",
  code: "Gerar código",
  apply: "Aplicar",
  done: "Concluído",
};

/** Groups missions by Kanban column. */
export function groupMissionsByColumn(
  missions: Mission[],
): Record<MissionColumnId, Mission[]> {
  const grouped: Record<MissionColumnId, Mission[]> = {
    todo: [],
    plan: [],
    code: [],
    apply: [],
    done: [],
  };
  for (const mission of missions) {
    const col = STATUS_TO_COLUMN[mission.status];
    grouped[col].push(mission);
  }
  // Sort within each column: active first, then by updatedAt desc
  const activeStatuses: MissionStatus[] = [
    "planning",
    "generating_code",
    "applying",
  ];
  for (const col of COLUMN_ORDER) {
    grouped[col].sort((a, b) => {
      const aActive = activeStatuses.includes(a.status);
      const bActive = activeStatuses.includes(b.status);
      if (aActive && !bActive) return -1;
      if (!aActive && bActive) return 1;
      return b.updatedAt.getTime() - a.updatedAt.getTime();
    });
  }
  return grouped;
}
