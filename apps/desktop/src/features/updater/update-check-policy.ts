export const UPDATE_CHECK_INTERVAL_MS = 30 * 60 * 1_000;

const UPDATE_CHECK_FOCUS_COOLDOWN_MS = 5 * 60 * 1_000;

export function automaticUpdateCheckIsDue(
	lastCheckStartedAt: number,
	now: number,
) {
	return now - lastCheckStartedAt >= UPDATE_CHECK_FOCUS_COOLDOWN_MS;
}
