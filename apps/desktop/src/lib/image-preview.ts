import { invoke } from "@tauri-apps/api/core";
import { useQuery } from "@tanstack/react-query";
import { DCC_QUERY_GC_TIME_MS } from "@/lib/query-client";

function isTauriRuntime() {
	return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/**
 * A local image the person attached, as a data URL for its thumbnail. `null`
 * when the file is gone (pasted images live in the OS temp dir) or the app
 * runs outside Tauri; callers fall back to the file chip.
 */
export function useImagePreview(path: string) {
	return useQuery({
		queryKey: ["imagePreview", path],
		queryFn: async () => {
			if (!isTauriRuntime()) return null;
			try {
				const result = await invoke<{ dataUrl: string }>("read_image_preview", { path });
				return result.dataUrl;
			} catch {
				return null;
			}
		},
		staleTime: Number.POSITIVE_INFINITY,
		gcTime: DCC_QUERY_GC_TIME_MS.heavyPayload,
		retry: false,
	});
}
