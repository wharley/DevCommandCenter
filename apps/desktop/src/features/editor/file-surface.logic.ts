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
