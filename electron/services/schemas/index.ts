/**
 * Zod schemas for internal canonical formats (MissionPlan, GeneratedCode).
 * Used by adapters to validate CLI/API output and fail fast with clear errors.
 */

import { z } from "zod";

export const planStepSchema = z.object({
  id: z.string().optional(),
  order: z.number(),
  title: z.string(),
  description: z.string(),
  files: z.array(z.string()).optional(),
  status: z.enum(["pending", "in_progress", "completed", "skipped"]).optional(),
});

export const missionPlanSchema = z.object({
  steps: z
    .array(planStepSchema)
    .min(1, "MissionPlan must have at least one step"),
  summary: z.string().optional(),
  estimatedComplexity: z.enum(["low", "medium", "high"]).optional(),
});

/**
 * Code suggestion schema with action-based validation (Pilar 3):
 * - create: suggestedContent is REQUIRED
 * - modify: suggestedContent is REQUIRED, diff is REQUIRED (for preview)
 * - delete: only path is required
 */
export const codeSuggestionSchema = z
  .object({
    path: z.string(),
    action: z.enum(["create", "modify", "delete"]),
    originalContent: z.string().optional(),
    suggestedContent: z.string().optional(),
    diff: z.string().optional(),
  })
  .refine(
    (data) => {
      // For "create" action, suggestedContent is required
      if (data.action === "create") {
        return !!data.suggestedContent;
      }
      return true;
    },
    {
      message: "suggestedContent is required for 'create' action",
      path: ["suggestedContent"],
    }
  )
  .refine(
    (data) => {
      // For "modify" action, suggestedContent is required (Pilar 3 - reliable apply)
      if (data.action === "modify") {
        return !!data.suggestedContent;
      }
      return true;
    },
    {
      message:
        "suggestedContent is required for 'modify' action (ensures reliable apply)",
      path: ["suggestedContent"],
    }
  );

export const generatedCodeSchema = z.object({
  files: z.array(codeSuggestionSchema).min(0),
  summary: z.string().optional(),
});

export type MissionPlanValidated = z.infer<typeof missionPlanSchema>;
export type GeneratedCodeValidated = z.infer<typeof generatedCodeSchema>;
export type CodeSuggestionValidated = z.infer<typeof codeSuggestionSchema>;
