export type WorkbenchCommand =
	| "composer.focus"
	| "composer.addContext"
	| "composer.execution"
	| "composer.togglePlan"
	| "terminal.toggle"
	| "terminal.openWorktree"
	| "terminal.openProject"
	| "terminal.newWorktree"
	| "inspector.changes"
	| "inspector.files"
	| "inspector.activity"
	| "inspector.details";

const WORKBENCH_COMMAND_EVENT = "dcc:workbench-command";

export function dispatchWorkbenchCommand(command: WorkbenchCommand): void {
	window.dispatchEvent(
		new CustomEvent<WorkbenchCommand>(WORKBENCH_COMMAND_EVENT, { detail: command }),
	);
}

export function subscribeWorkbenchCommand(
	listener: (command: WorkbenchCommand) => void,
): () => void {
	const handler = (event: Event) => {
		listener((event as CustomEvent<WorkbenchCommand>).detail);
	};
	window.addEventListener(WORKBENCH_COMMAND_EVENT, handler);
	return () => window.removeEventListener(WORKBENCH_COMMAND_EVENT, handler);
}
