import { describe, expect, it } from "vitest";
import {
	repositoryDisplayName,
	repositoryHasCustomDisplayName,
} from "./repository-display-name";

const repository = {
	displayName: null,
	name: "technical-repo",
	rootPath: "/projects/technical-repo",
};

describe("repository display name", () => {
	it("prefers the user-facing project name", () => {
		expect(
			repositoryDisplayName({ ...repository, displayName: "Customer Portal" }),
		).toBe("Customer Portal");
	});

	it("falls back to the stable technical repository name", () => {
		expect(repositoryDisplayName(repository)).toBe("technical-repo");
	});

	it("treats whitespace as no customization", () => {
		expect(
			repositoryHasCustomDisplayName({ ...repository, displayName: "   " }),
		).toBe(false);
	});
});
