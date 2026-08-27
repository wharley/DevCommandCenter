export type FileSurfaceContentState = "error" | "loading" | "editor";

export type FileSurfaceQueryFlags = {
	isError: boolean;
	isPending: boolean;
};

export type FileSurfaceTabState = {
	dirty: boolean;
	saving: boolean;
};

export function resolveFileSurfaceContentState({
	isError,
	isPending,
}: FileSurfaceQueryFlags): FileSurfaceContentState {
	if (isError) {
		return "error";
	}
	if (isPending) {
		return "loading";
	}
	return "editor";
}

export function hasDirtyFileSurfaceState(
	stateByPath: Record<string, FileSurfaceTabState>,
) {
	return Object.values(stateByPath).some((state) => state.dirty);
}

/**
 * The editor invokes this only after its own dirty-buffer confirmation. Prefer
 * the direct completion callback so it does not re-enter an external close
 * transition and ask the user a second time.
 */
export function resolveConfirmedFileSurfaceCloseHandler({
	onFileSurfaceClosed,
	onCloseSurface,
}: {
	onFileSurfaceClosed?: () => void;
	onCloseSurface: () => void;
}) {
	return onFileSurfaceClosed ?? onCloseSurface;
}

/**
 * Clean inactive file surfaces are reproducible from their externalized buffer
 * and query data, so keeping their observers mounted only retains large payloads.
 * Dirty and saving tabs stay alive to preserve conflict reconciliation and writes.
 */
export function shouldKeepFileSurfaceMounted(
	filePath: string,
	activePath: string,
	state?: FileSurfaceTabState,
) {
	return filePath === activePath || Boolean(state?.dirty || state?.saving);
}
