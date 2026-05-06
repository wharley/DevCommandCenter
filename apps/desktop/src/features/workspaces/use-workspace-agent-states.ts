import { useEffect, useRef, useState } from "react";
import type { CoreEvent } from "@dcc/contracts";
import { listenSessionEvents } from "@/lib/session-api";

export type AgentState = "active" | "completed" | "aborted";

const CLEAR_DELAY_MS = 8_000;

export function useWorkspaceAgentStates(): Record<string, AgentState> {
	const [agentStates, setAgentStates] = useState<Record<string, AgentState>>({});
	const sessionsRef = useRef<Record<string, string>>({});
	const timeoutsRef = useRef<Map<string, ReturnType<typeof setTimeout>>>(new Map());

	useEffect(() => {
		const timeouts = timeoutsRef.current;
		let unlistenFn: (() => void) | null = null;

		function scheduleAutoClear(workspaceId: string) {
			const existing = timeouts.get(workspaceId);
			if (existing) clearTimeout(existing);

			const id = setTimeout(() => {
				setAgentStates((prev) => {
					const { [workspaceId]: _, ...rest } = prev;
					return rest;
				});
				timeouts.delete(workspaceId);
			}, CLEAR_DELAY_MS);

			timeouts.set(workspaceId, id);
		}

		void listenSessionEvents((event: CoreEvent) => {
			if (event.sessionStarted) {
				const { session_id, workspace_id } = event.sessionStarted;
				sessionsRef.current[session_id] = workspace_id;

				const existing = timeouts.get(workspace_id);
				if (existing) {
					clearTimeout(existing);
					timeouts.delete(workspace_id);
				}

				setAgentStates((prev) => ({ ...prev, [workspace_id]: "active" }));
			} else if (event.sessionCompleted) {
				const workspaceId = sessionsRef.current[event.sessionCompleted.session_id];
				if (workspaceId) {
					setAgentStates((prev) => ({ ...prev, [workspaceId]: "completed" }));
					scheduleAutoClear(workspaceId);
				}
			} else if (event.sessionAborted) {
				const workspaceId = sessionsRef.current[event.sessionAborted.session_id];
				if (workspaceId) {
					setAgentStates((prev) => ({ ...prev, [workspaceId]: "aborted" }));
					scheduleAutoClear(workspaceId);
				}
			}
		}).then((fn) => {
			unlistenFn = fn;
		});

		return () => {
			unlistenFn?.();
			for (const timeout of timeouts.values()) clearTimeout(timeout);
			timeouts.clear();
		};
	}, []);

	return agentStates;
}
