/** Prefix of the backend error that refuses to delete unapplied child-task edits. */
export const UNAPPLIED_DELEGATION_EDITS = "unapplied_delegation_edits:";

export type UnappliedDelegationEdits = {
	title: string;
	files: number;
};

/**
 * Reads the backend refusal. `list` is set when the task's delete listed the
 * child tasks it would lose edits from; `blocked` alone when another cleanup
 * (a bundle, a project) hit the same refusal without the list.
 */
export function parseUnappliedDelegationEdits(message: string): {
	blocked: boolean;
	list: UnappliedDelegationEdits[] | null;
} {
	const index = message.indexOf(UNAPPLIED_DELEGATION_EDITS);
	if (index < 0) {
		return { blocked: false, list: null };
	}
	try {
		const parsed: unknown = JSON.parse(
			message.slice(index + UNAPPLIED_DELEGATION_EDITS.length),
		);
		if (
			Array.isArray(parsed) &&
			parsed.length > 0 &&
			parsed.every(
				(item) =>
					typeof item?.title === "string" && typeof item?.files === "number",
			)
		) {
			return { blocked: true, list: parsed as UnappliedDelegationEdits[] };
		}
	} catch {
		// Not the delete's list; still a refusal.
	}
	return { blocked: true, list: null };
}
