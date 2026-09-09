/** Unwrap provider error envelopes without guessing or translating the cause. */
export function providerErrorMessage(reason: string): string {
	function extract(value: unknown, depth: number): string | null {
		if (depth > 8) return null;
		if (typeof value === "string") {
			if (!value.trim()) return null;
			try {
				return extract(JSON.parse(value), depth + 1) ?? value;
			} catch {
				return value;
			}
		}
		if (!value || typeof value !== "object" || Array.isArray(value)) return null;
		const record = value as Record<string, unknown>;
		return extract(record.error, depth + 1) ?? extract(record.message, depth + 1);
	}
	return extract(reason, 0) ?? reason;
}
