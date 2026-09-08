/** Release the signal listener on every settlement, including turn cleanup. */
export function waitForPendingResponse(pending, requestId, signal, abortedValue) {
	return new Promise((resolve) => {
		let settled = false;
		const settle = (value, aborted = false) => {
			if (settled) return;
			settled = true;
			signal.removeEventListener("abort", onAbort);
			if (pending.get(requestId) === request) pending.delete(requestId);
			resolve({ value, aborted });
		};
		const onAbort = () => settle(abortedValue, true);
		const request = { resolve: (value) => settle(value) };
		pending.set(requestId, request);
		signal.addEventListener("abort", onAbort, { once: true });
		// Abort events are not replayed when a listener is added after cancellation.
		if (signal.aborted) onAbort();
	});
}
