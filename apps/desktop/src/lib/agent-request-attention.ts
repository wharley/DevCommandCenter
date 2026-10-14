import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";

function isTauriRuntime(): boolean {
	return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/**
 * Calls for the person once per request waiting on them: when DCC is not the
 * window in front, its Dock icon bounces and a system notification names the
 * request. `requestId` null means nothing is waiting on the person.
 */
export function useAgentRequestAttention(
	requestId: string | null,
	title: string,
	body: string,
) {
	const announced = useRef(new Set<string>());
	useEffect(() => {
		if (!requestId || announced.current.has(requestId) || !isTauriRuntime()) return;
		announced.current.add(requestId);
		void invoke("agent_request_attention", { title, body }).catch(() => {});
	}, [requestId, title, body]);
}
