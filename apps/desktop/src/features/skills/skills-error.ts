export function skillsErrorMessage(error: unknown, fallback: string) {
	if (typeof error === "string" && error.trim()) {
		return error;
	}
	if (error instanceof Error && error.message.trim()) {
		return error.message;
	}
	if (
		typeof error === "object" &&
		error !== null &&
		"message" in error &&
		typeof error.message === "string" &&
		error.message.trim()
	) {
		return error.message;
	}
	return fallback;
}
