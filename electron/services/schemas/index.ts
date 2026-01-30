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
  status: z
    .enum(["pending", "in_progress", "completed", "skipped"])
    .optional(),
});

export const missionPlanSchema = z.object({
  steps: z.array(planStepSchema).min(1, "MissionPlan must have at least one step"),
  summary: z.string().optional(),
  estimatedComplexity: z.enum(["low", "medium", "high"]).optional(),
});

export const codeSuggestionSchema = z.object({
  path: z.string(),
  action: z.enum(["create", "modify", "delete"]),
  originalContent: z.string().optional(),
  suggestedContent: z.string().optional(),
  diff: z.string().optional(),
});

export const generatedCodeSchema = z.object({
  files: z.array(codeSuggestionSchema).min(1, "GeneratedCode must have at least one file"),
  summary: z.string().optional(),
});

export type MissionPlanValidated = z.infer<typeof missionPlanSchema>;
export type GeneratedCodeValidated = z.infer<typeof generatedCodeSchema>;
export type CodeSuggestionValidated = z.infer<typeof codeSuggestionSchema>;
