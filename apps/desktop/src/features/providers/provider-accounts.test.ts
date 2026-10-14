import { describe, expect, it } from "vitest";
import type { ProviderAccount } from "@dcc/contracts";
import {
	accountsOfProvider,
	parseProviderAccountSelection,
	resolveTurnAccountId,
	shownAccountId,
	withProviderAccount,
} from "./provider-accounts";

const account = (id: string, providerId = "claude_code", createdAt = "2026-10-14T10:00:00Z") =>
	({
		id,
		providerId,
		label: id,
		configDir: `/accounts/${id}`,
		managed: true,
		createdAt,
	}) as ProviderAccount;

const zedy = account("claude_code-zedy", "claude_code", "2026-10-14T11:00:00Z");
const personal = account("claude_code-personal", "claude_code", "2026-10-14T10:00:00Z");
const accounts = [zedy, personal, account("codex-work", "codex")];

describe("provider accounts", () => {
	it("lists one provider's accounts in the order they were added", () => {
		expect(accountsOfProvider(accounts, "claude_code").map((entry) => entry.id)).toEqual([
			"claude_code-personal",
			"claude_code-zedy",
		]);
	});

	it("keeps an open conversation on its own account", () => {
		const conversation = { providerId: "claude_code", accountId: "claude_code-zedy" };
		const selection = { claude_code: "claude_code-personal" };
		expect(
			resolveTurnAccountId({ providerId: "claude_code", conversation, selection, accounts }),
		).toBe("claude_code-zedy");
		expect(shownAccountId("claude_code", conversation, selection, accounts)).toEqual({
			accountId: "claude_code-zedy",
			locked: true,
		});
	});

	it("uses the picked account for new conversations and drops removed ones", () => {
		expect(
			resolveTurnAccountId({
				providerId: "claude_code",
				conversation: { providerId: "codex", accountId: null },
				selection: { claude_code: "claude_code-personal" },
				accounts,
			}),
		).toBe("claude_code-personal");
		expect(
			resolveTurnAccountId({
				providerId: "claude_code",
				conversation: null,
				selection: { claude_code: "claude_code-gone" },
				accounts,
			}),
		).toBeNull();
		expect(
			resolveTurnAccountId({
				providerId: "claude_code",
				conversation: null,
				selection: { claude_code: "codex-work" },
				accounts,
			}),
		).toBeNull();
	});

	it("adds the account to the runtime without dropping other settings", () => {
		expect(withProviderAccount(null, null)).toBeNull();
		expect(withProviderAccount({ binaryPath: "/bin/claude" }, "claude_code-zedy")).toEqual({
			binaryPath: "/bin/claude",
			accountId: "claude_code-zedy",
		});
	});

	it("ignores malformed stored selections", () => {
		expect(parseProviderAccountSelection("not json")).toEqual({});
		expect(parseProviderAccountSelection("[1]")).toEqual({});
		expect(
			parseProviderAccountSelection('{"claude_code":"claude_code-zedy","codex":3,"x":""}'),
		).toEqual({ claude_code: "claude_code-zedy" });
	});
});
