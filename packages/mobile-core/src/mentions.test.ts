import { describe, expect, it } from "vitest";
import { applyFileMention, fileSearchPath, filterTrackedFiles, findActiveFileMention, splitPath } from "./mentions.ts";

describe("file mentions", () => {
	it("opens on an @ at the start, after a space or a parenthesis", () => {
		expect(findActiveFileMention("@src/ma", 7)).toEqual({ start: 0, end: 7, query: "src/ma" });
		expect(findActiveFileMention("olhe @", 6)).toEqual({ start: 5, end: 6, query: "" });
		expect(findActiveFileMention("veja (@app/[token]/route.ts", 27)?.query).toBe("app/[token]/route.ts");
	});

	it("ignores e-mails, finished mentions and text after the cursor", () => {
		expect(findActiveFileMention("mande para ana@empresa.com", 26)).toBeNull();
		expect(findActiveFileMention("@src/main.ts e depois", 21)).toBeNull();
		expect(findActiveFileMention("@src e", 4)).toEqual({ start: 0, end: 4, query: "src" });
		expect(findActiveFileMention("sem arroba", 10)).toBeNull();
	});

	it("inserts @path and a single space", () => {
		const text = "corrija @ma e rode";
		const mention = findActiveFileMention(text, 11)!;
		expect(applyFileMention(text, mention, "src/main.ts")).toEqual({
			text: "corrija @src/main.ts e rode",
			cursor: 21,
		});
		expect(applyFileMention("@", { start: 0, end: 1, query: "" }, "README.md")).toEqual({
			text: "@README.md ",
			cursor: 11,
		});
	});

	it("searches by id, never by path", () => {
		expect(fileSearchPath({ workspaceId: "w 1" }, "src/a b")).toBe("/api/v1/mobile/workspaces/w%201/files?q=src%2Fa%20b");
		expect(fileSearchPath({ repositoryId: "r" }, "")).toBe("/api/v1/mobile/repositories/r/files?q=");
		expect(splitPath("src/ui/Button.tsx")).toEqual({ name: "Button.tsx", dir: "src/ui/" });
		expect(splitPath("README.md")).toEqual({ name: "README.md", dir: "" });
	});

	it("ranks like the desktop", () => {
		const paths = ["README.md", "src/app/payment.ts", "src/app/paymentPix.ts", "docs/payment-flow.md", "src/PayButton.tsx"];
		expect(filterTrackedFiles(paths, "payment.ts")).toEqual(["src/app/payment.ts"]);
		expect(filterTrackedFiles(paths, "PAY")).toEqual(["src/app/payment.ts", "src/app/paymentPix.ts", "docs/payment-flow.md", "src/PayButton.tsx"]);
		expect(filterTrackedFiles(paths, "", 2)).toEqual(["README.md", "src/app/payment.ts"]);
	});
});
