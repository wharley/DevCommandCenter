import type {
	WorkspaceMessage,
	WorkspaceMessageAnnotation,
} from "./thread-projection";

export type PendingPermissionRequest = Extract<
	WorkspaceMessageAnnotation,
	{ type: "approval" }
>;

export function collectPendingPermissionRequests(
	messages: WorkspaceMessage[],
): PendingPermissionRequest[] {
	const pending: PendingPermissionRequest[] = [];
	const seenRequestIds = new Set<string>();

	for (const message of messages) {
		for (const annotation of message.annotations ?? []) {
			if (
				annotation.type !== "approval" ||
				!annotation.streaming ||
				annotation.behavior ||
				seenRequestIds.has(annotation.id)
			) {
				continue;
			}
			seenRequestIds.add(annotation.id);
			pending.push(annotation);
		}
	}

	return pending;
}
