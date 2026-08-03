import { describe, expect, it } from "vitest";
import { composerTurnFromRaw, DEFAULT_COMPOSER_ENVELOPE } from "./composer-turn";

describe("composer turn defaults", () => {
	it("uses the standard response style unless the user selects direct", () => {
		expect(DEFAULT_COMPOSER_ENVELOPE.fastMode).toBe(false);
		expect(composerTurnFromRaw("Inspect the project").envelope.fastMode).toBe(false);
		expect(
			composerTurnFromRaw("Inspect the project", { fastMode: true }).envelope.fastMode,
		).toBe(true);
	});
});
