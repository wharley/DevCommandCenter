export const DCC_UX_METRICS_STORAGE_KEY = "dcc-ux-metrics-v1";

export type DccUxMetricName =
	| "first_prompt"
	| "diff_discovered"
	| "terminal_discovered"
	| "terminal_scope_switched"
	| "advanced_composer_control_used"
	| "steer_prompt"
	| "queue_prompt"
	| "command_palette_action";

export type DccUxMetric = {
	count: number;
	firstElapsedMs: number;
	lastElapsedMs: number;
};

export type DccUxMetrics = Partial<Record<DccUxMetricName, DccUxMetric>>;

const sessionStartedAt = Date.now();

export function readUxMetrics(): DccUxMetrics {
	if (typeof window === "undefined") return {};
	try {
		const parsed = JSON.parse(
			window.localStorage.getItem(DCC_UX_METRICS_STORAGE_KEY) ?? "{}",
		) as DccUxMetrics;
		return parsed && typeof parsed === "object" ? parsed : {};
	} catch {
		return {};
	}
}

/** Stores only aggregate counts and elapsed times; never prompts, paths or IDs. */
export function recordUxMetric(name: DccUxMetricName): void {
	if (typeof window === "undefined") return;
	const elapsed = Math.max(0, Date.now() - sessionStartedAt);
	const current = readUxMetrics();
	const previous = current[name];
	current[name] = {
		count: (previous?.count ?? 0) + 1,
		firstElapsedMs: previous?.firstElapsedMs ?? elapsed,
		lastElapsedMs: elapsed,
	};
	try {
		window.localStorage.setItem(DCC_UX_METRICS_STORAGE_KEY, JSON.stringify(current));
	} catch {
		/* localStorage unavailable */
	}
}
