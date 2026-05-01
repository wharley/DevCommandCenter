import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { SESSION_METHODS } from "@dcc/contracts";
import type { CoreEvent } from "@dcc/contracts";
import type {
	AbortRunInput,
	AbortRunOutput,
	ResumeSessionInput,
	ResumeSessionOutput,
	SendTurnInput,
	SendTurnOutput,
	StartThreadInput,
	StartThreadOutput,
} from "@dcc/contracts";

export function startThread(input: StartThreadInput) {
	return invoke<StartThreadOutput>(SESSION_METHODS.startThread, { input });
}

export function sendTurn(input: SendTurnInput) {
	return invoke<SendTurnOutput>(SESSION_METHODS.sendTurn, { input });
}

export function abortRun(input: AbortRunInput) {
	return invoke<AbortRunOutput>(SESSION_METHODS.abortRun, { input });
}

export function resumeSession(input: ResumeSessionInput) {
	return invoke<ResumeSessionOutput>(SESSION_METHODS.resumeSession, { input });
}

const SESSION_EVENT_NAMES = [
	"dcc.session.started",
	"dcc.session.completed",
	"dcc.session.aborted",
	"dcc.session.resumed",
	"dcc.session.turn.started",
	"dcc.session.turn.completed",
	"dcc.session.turn.aborted",
	"dcc.session.checkpoint.created",
] as const;

export async function listenSessionEvents(
	handler: (event: CoreEvent) => void,
) {
	const unlistenFns = await Promise.all(
		SESSION_EVENT_NAMES.map((eventName) => listen<CoreEvent>(eventName, (event) => {
			handler(event.payload);
		})),
	);

	return () => {
		for (const unlisten of unlistenFns) {
			void unlisten();
		}
	};
}
