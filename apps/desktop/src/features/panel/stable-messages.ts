import { useRef } from "react";
import type { WorkspaceMessage } from "./thread-projection";

/** Structural equality for plain projection data (no Maps, Dates or cycles). */
export function sameProjectionValue(left: unknown, right: unknown): boolean {
	if (Object.is(left, right)) return true;
	if (typeof left !== "object" || typeof right !== "object" || !left || !right) return false;
	if (Array.isArray(left)) {
		if (!Array.isArray(right) || left.length !== right.length) return false;
		for (let index = 0; index < left.length; index += 1) {
			if (!sameProjectionValue(left[index], right[index])) return false;
		}
		return true;
	}
	if (Array.isArray(right)) return false;
	const leftRecord = left as Record<string, unknown>;
	const rightRecord = right as Record<string, unknown>;
	const leftKeys = Object.keys(leftRecord);
	if (leftKeys.length !== Object.keys(rightRecord).length) return false;
	for (const key of leftKeys) {
		if (!sameProjectionValue(leftRecord[key], rightRecord[key])) return false;
	}
	return true;
}

/**
 * Re-projection builds every message object anew on each live frame. Reusing
 * the previous object when nothing changed lets memoized rows skip rendering,
 * so a streaming turn re-renders itself and not the whole transcript.
 */
export function stabilizeMessages(
	previous: ReadonlyMap<string, WorkspaceMessage>,
	next: readonly WorkspaceMessage[],
): { messages: WorkspaceMessage[]; byId: Map<string, WorkspaceMessage>; changed: boolean } {
	const byId = new Map<string, WorkspaceMessage>();
	let changed = previous.size !== next.length;
	const messages = next.map((message) => {
		const prior = previous.get(message.id);
		const stable = prior && sameProjectionValue(prior, message) ? prior : message;
		if (stable !== prior) changed = true;
		byId.set(message.id, stable);
		return stable;
	});
	return { messages, byId, changed };
}

export function useStableMessages(messages: WorkspaceMessage[]): WorkspaceMessage[] {
	const stateRef = useRef<{ byId: Map<string, WorkspaceMessage>; messages: WorkspaceMessage[] }>({
		byId: new Map(),
		messages: [],
	});
	const result = stabilizeMessages(stateRef.current.byId, messages);
	if (result.changed || result.messages.length !== stateRef.current.messages.length) {
		stateRef.current = { byId: result.byId, messages: result.messages };
	} else {
		// Same objects in the same order: keep the previous array identity too.
		const same = result.messages.every((message, index) => message === stateRef.current.messages[index]);
		if (!same) stateRef.current = { byId: result.byId, messages: result.messages };
	}
	return stateRef.current.messages;
}
