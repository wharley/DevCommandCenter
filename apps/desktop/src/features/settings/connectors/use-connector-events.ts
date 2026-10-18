import { useEffect } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { MCP_INTEGRATIONS_QUERY_KEY } from "../mcp-integrations-panel";

function isTauriRuntime(): boolean {
	return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/**
 * Refreshes the connector list when the gateway reports a lost sign-in while
 * an agent was using a connector.
 */
export function useConnectorNeedsAuthEvents() {
	const queryClient = useQueryClient();
	useEffect(() => {
		if (!isTauriRuntime()) return;
		let disposed = false;
		let unlisten: (() => void) | undefined;
		void listen<string>("mcp-connector-needs-auth", () => {
			void queryClient.invalidateQueries({ queryKey: MCP_INTEGRATIONS_QUERY_KEY });
		})
			.then((stop) => {
				if (disposed) stop();
				else unlisten = stop;
			})
			.catch(() => undefined);
		return () => {
			disposed = true;
			unlisten?.();
		};
	}, [queryClient]);
}
