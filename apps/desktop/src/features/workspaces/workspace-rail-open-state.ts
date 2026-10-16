import { COMPLETED_SECTION_ID, WAITING_SECTION_ID } from "./workspace-rail-shared";

export const DCC_WORKBENCH_RAIL_SECTION_STATE_KEY =
	"dcc.workbenchRail.sectionOpenState";

export function createInitialRailSectionState() {
	return Object.fromEntries([
		[WAITING_SECTION_ID, false],
		[COMPLETED_SECTION_ID, false],
	]) as Record<string, boolean>;
}

export function readStoredRailSectionState(): Record<string, boolean> | null {
	if (typeof window === "undefined") {
		return null;
	}

	try {
		const raw = window.localStorage.getItem(DCC_WORKBENCH_RAIL_SECTION_STATE_KEY);
		if (!raw) {
			return null;
		}

		const parsed = JSON.parse(raw) as unknown;
		if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
			return null;
		}

		const stored = parsed as Record<string, boolean>;
		return {
			...stored,
			[WAITING_SECTION_ID]:
				stored[WAITING_SECTION_ID] ?? stored.dccArchived ?? false,
			[COMPLETED_SECTION_ID]: stored[COMPLETED_SECTION_ID] ?? false,
		};
	} catch {
		return null;
	}
}

export function writeStoredRailSectionState(state: Record<string, boolean>) {
	if (typeof window === "undefined") {
		return;
	}

	try {
		window.localStorage.setItem(
			DCC_WORKBENCH_RAIL_SECTION_STATE_KEY,
			JSON.stringify(state),
		);
	} catch (error) {
		console.error(
			`[dcc] failed to persist workbench rail section state (${DCC_WORKBENCH_RAIL_SECTION_STATE_KEY})`,
			error,
		);
	}
}
