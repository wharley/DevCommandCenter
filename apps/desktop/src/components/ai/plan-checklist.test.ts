import { describe, expect, it } from "vitest";
import { parsePlanChecklist } from "./plan-checklist";

describe("parsePlanChecklist", () => {
	it("reads Claude's TodoWrite and names the running step in the present tense", () => {
		const checklist = parsePlanChecklist(
			JSON.stringify({
				todos: [
					{ content: "Read the code", status: "completed", activeForm: "Reading the code" },
					{ content: "Fix the bug", status: "in_progress", activeForm: "Fixing the bug" },
					{ content: "Run the tests", status: "pending", activeForm: "Running the tests" },
				],
			}),
		);
		expect(checklist?.completed).toBe(1);
		expect(checklist?.steps).toHaveLength(3);
		expect(checklist?.current).toEqual({ text: "Fixing the bug", status: "in_progress" });
	});

	it("reads Codex's plan update and todo list", () => {
		expect(
			parsePlanChecklist(
				JSON.stringify({
					plan: [
						{ step: "Inspect", status: "completed" },
						{ step: "Change", status: "inProgress" },
					],
				}),
			)?.current,
		).toEqual({ text: "Change", status: "in_progress" });
		const todo = parsePlanChecklist(
			JSON.stringify({
				items: [
					{ text: "One", completed: true },
					{ text: "Two", completed: false },
				],
			}),
		);
		expect(todo?.completed).toBe(1);
		expect(todo?.current).toEqual({ text: "Two", status: "pending" });
	});

	it("has no current step once every step is done", () => {
		expect(
			parsePlanChecklist(JSON.stringify({ todos: [{ content: "Only", status: "completed" }] }))
				?.current,
		).toBeNull();
	});

	it("is not a checklist for other input, broken JSON or empty lists", () => {
		expect(parsePlanChecklist(undefined)).toBeNull();
		expect(parsePlanChecklist("{not json")).toBeNull();
		expect(parsePlanChecklist(JSON.stringify({ command: "ls" }))).toBeNull();
		expect(parsePlanChecklist(JSON.stringify({ todos: [] }))).toBeNull();
	});
});
