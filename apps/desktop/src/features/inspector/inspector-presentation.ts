export type InspectorPresentation = "contextual" | "pinned";

export function shouldCollapseContextualInspector(
	presentation: InspectorPresentation,
	collapsed: boolean,
): boolean {
	return presentation === "contextual" && !collapsed;
}
