import { describe, expect, it } from "vitest";

import { parseUnappliedDelegationEdits } from "./unapplied-delegation-edits";

describe("parseUnappliedDelegationEdits", () => {
	it("reads the child tasks a delete would lose edits from", () => {
		expect(
			parseUnappliedDelegationEdits(
				'unapplied_delegation_edits:[{"title":"Corrigir domínio","files":8}]',
			),
		).toEqual({ blocked: true, list: [{ title: "Corrigir domínio", files: 8 }] });
	});

	it("recognizes the refusal inside a wrapped error without a list", () => {
		expect(
			parseUnappliedDelegationEdits(
				"multi-workspace delegation cleanup was incomplete: ws delegations: unapplied_delegation_edits:3 uncommitted file(s)",
			),
		).toEqual({ blocked: true, list: null });
	});

	it("leaves other errors alone", () => {
		expect(parseUnappliedDelegationEdits("git worktree remove failed")).toEqual({
			blocked: false,
			list: null,
		});
	});
});
