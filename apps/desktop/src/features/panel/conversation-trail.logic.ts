import type { WorkspaceMessage } from "./thread-projection";

export type ConversationTrailItem = {
	id: string;
	ordinal: number;
	promptPreview: string;
	responsePreview: string;
};

const MAX_PREVIEW_LENGTH = 280;

function normalizePreview(content: string): string {
	const collapsed = content.replace(/\s+/g, " ").trim();
	return collapsed.length > MAX_PREVIEW_LENGTH
		? `${collapsed.slice(0, MAX_PREVIEW_LENGTH).trimEnd()}…`
		: collapsed;
}

/**
 * Projects a conversation into one navigation stop per user turn. Assistant
 * previews are paired by turn id when available, with transcript order as a
 * fallback for older histories that do not carry turn ids.
 */
export function deriveConversationTrailItems(
	messages: readonly WorkspaceMessage[],
): ConversationTrailItem[] {
	const items: ConversationTrailItem[] = [];
	const itemIndexByTurnId = new Map<string, number>();
	let latestUserItemIndex = -1;

	for (const message of messages) {
		if (message.role === "user") {
			const itemIndex = items.length;
			items.push({
				id: message.id,
				ordinal: itemIndex + 1,
				promptPreview: normalizePreview(message.content),
				responsePreview: "",
			});
			latestUserItemIndex = itemIndex;
			if (message.turnId) {
				itemIndexByTurnId.set(message.turnId, itemIndex);
			}
			continue;
		}

		if (message.role !== "assistant") {
			continue;
		}

		const responsePreview = normalizePreview(message.content);
		if (!responsePreview) {
			continue;
		}

		const matchingIndex = message.turnId
			? itemIndexByTurnId.get(message.turnId)
			: undefined;
		const itemIndex = matchingIndex ?? latestUserItemIndex;
		if (itemIndex >= 0) {
			items[itemIndex]!.responsePreview = responsePreview;
		}
	}

	return items;
}

export function focusedTrailIndex(
	pointerY: number,
	count: number,
	spacing: number,
): number {
	if (count <= 1 || spacing <= 0) {
		return 0;
	}
	return Math.max(0, Math.min(count - 1, Math.round(pointerY / spacing)));
}

export function trailMagnificationWeights(
	count: number,
	pointerY: number,
	spacing: number,
	sigma: number,
): number[] {
	if (count <= 0) {
		return [];
	}
	if (spacing <= 0 || sigma <= 0) {
		const focusedIndex = focusedTrailIndex(pointerY, count, spacing);
		return Array.from({ length: count }, (_, index) =>
			index === focusedIndex ? 1 : 0,
		);
	}
	const denominator = 2 * sigma * sigma;
	return Array.from({ length: count }, (_, index) => {
		const distance = index * spacing - pointerY;
		return Math.exp(-(distance * distance) / denominator);
	});
}
