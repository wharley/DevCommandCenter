import { useSyncExternalStore } from "react";

const listeners = new Map<string, Set<() => void>>();
const busyRoots = new Set<string>();

function normalizedRoot(root: string | null | undefined) {
	return root?.trim() ?? "";
}

function notify(root: string) {
	listeners.get(root)?.forEach((listener) => listener());
}

export function setWorkspaceDeliveryBusy(root: string | null | undefined, busy: boolean) {
	const normalized = normalizedRoot(root);
	if (!normalized) return;

	if (busy) {
		busyRoots.add(normalized);
	} else {
		busyRoots.delete(normalized);
	}
	notify(normalized);
}

export function isWorkspaceDeliveryBusy(root: string | null | undefined) {
	const normalized = normalizedRoot(root);
	return normalized ? busyRoots.has(normalized) : false;
}

export function useWorkspaceDeliveryBusy(root: string | null | undefined) {
	const normalized = normalizedRoot(root);
	return useSyncExternalStore(
		(listener) => {
			if (!normalized) return () => undefined;
			const rootListeners = listeners.get(normalized) ?? new Set<() => void>();
			rootListeners.add(listener);
			listeners.set(normalized, rootListeners);
			return () => {
				rootListeners.delete(listener);
				if (rootListeners.size === 0) listeners.delete(normalized);
			};
		},
		() => isWorkspaceDeliveryBusy(normalized),
		() => false,
	);
}
