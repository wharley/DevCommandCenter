export function finishTurn(state, terminalResult, emit) {
	for (const pending of state.pendingUserInputs.values()) {
		pending.resolve([]);
	}
	state.pendingUserInputs.clear();
	for (const pending of state.pendingPermissions.values()) {
		pending.resolve("deny");
	}
	state.pendingPermissions.clear();
	state.running = false;
	state.activeTurnPromise = null;
	emit(terminalResult);
}
