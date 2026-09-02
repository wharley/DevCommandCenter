import { describe, expect, it } from "vitest";
import type { ProviderCatalog } from "@dcc/contracts";
import {
	canDelegateEdits,
	delegationTargetsFor,
	eligibleDelegationTargets,
} from "./delegation-targets";

type Provider = ProviderCatalog["providers"][number];

function provider(
	id: string,
	capabilities: {
		canBeDelegationTarget: boolean;
		supportsReadOnlyDelegation: boolean;
		supportsEditDelegation: boolean;
	},
): Provider {
	return {
		id,
		label: id,
		models: [],
		capabilities,
	} as unknown as Provider;
}

const readOnly = provider("gemini", {
	canBeDelegationTarget: true,
	supportsReadOnlyDelegation: true,
	supportsEditDelegation: false,
});
const editCapable = provider("codex", {
	canBeDelegationTarget: true,
	supportsReadOnlyDelegation: true,
	supportsEditDelegation: true,
});
const notATarget = provider("local", {
	canBeDelegationTarget: false,
	supportsReadOnlyDelegation: true,
	supportsEditDelegation: true,
});
const disabled = {
	...editCapable,
	id: "disabled",
	enabled: false,
};
const catalog = [readOnly, editCapable, notATarget, disabled];

describe("eligibleDelegationTargets", () => {
	it("drops providers that cannot receive delegated work", () => {
		expect(eligibleDelegationTargets(catalog).map((p) => p.id)).toEqual([
			"gemini",
			"codex",
		]);
	});
});

describe("delegationTargetsFor", () => {
	it("keeps every eligible provider for read-only work", () => {
		expect(
			delegationTargetsFor(catalog, { allowFileEdits: false }).map((p) => p.id),
		).toEqual(["gemini", "codex"]);
	});

	it("narrows to edit-capable providers when file edits are allowed", () => {
		expect(
			delegationTargetsFor(catalog, { allowFileEdits: true }).map((p) => p.id),
		).toEqual(["codex"]);
	});
});

describe("canDelegateEdits", () => {
	it("is false for missing or read-only targets", () => {
		expect(canDelegateEdits(null)).toBe(false);
		expect(canDelegateEdits(readOnly)).toBe(false);
		expect(canDelegateEdits(editCapable)).toBe(true);
	});
});
