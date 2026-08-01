import { describe, expect, it } from "vitest";
import { setupDisplayCommand } from "./workspace-setup-report";

describe("setupDisplayCommand", () => {
	it("keeps the recommended install command readable when runtime activation is wrapped", () => {
		expect(
			setupDisplayCommand(
				"if command -v nvm >/dev/null 2>&1; then nvm use && corepack pnpm install; fi",
			),
		).toBe("corepack pnpm install");
	});
});
