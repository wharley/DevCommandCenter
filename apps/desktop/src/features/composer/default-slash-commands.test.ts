import { describe, expect, it } from "vitest";
import { DEFAULT_SLASH_COMMANDS, availableSlashCommands } from "./default-slash-commands";

describe("availableSlashCommands", () => {
	it("offers capability-bound commands only when the provider declares them", () => {
		const names = (capabilities: Parameters<typeof availableSlashCommands>[1]) =>
			availableSlashCommands(DEFAULT_SLASH_COMMANDS, capabilities).map((entry) => entry.name);
		expect(names({ supportsCompactionCommand: true })).toEqual(["spec", "clear", "compact"]);
		expect(names({ supportsCompactionCommand: false })).toEqual(["spec", "clear"]);
		expect(names({})).toEqual(["spec", "clear"]);
		expect(names(null)).toEqual(["spec", "clear"]);
	});
});
