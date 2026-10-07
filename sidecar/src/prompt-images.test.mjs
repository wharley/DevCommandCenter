import assert from "node:assert/strict";
import { test } from "node:test";
import { promptImageBlocks, referencedImagePaths } from "./prompt-images.mjs";

test("finds absolute image references once, in order", () => {
	assert.deepEqual(
		referencedImagePaths("look @/tmp/a.png and @/tmp/b.JPG then @/tmp/a.png @rel/c.png @/tmp/d.txt"),
		["/tmp/a.png", "/tmp/b.JPG"],
	);
	assert.deepEqual(referencedImagePaths(undefined), []);
});

test("builds base64 blocks and skips missing or oversized files", async () => {
	const files = {
		"/tmp/a.png": { size: 3, data: Buffer.from("abc") },
		"/tmp/big.png": { size: 6 * 1024 * 1024, data: Buffer.alloc(1) },
	};
	const fs = {
		stat: async (path) => {
			const file = files[path];
			if (!file) throw new Error("missing");
			return { size: file.size, isFile: () => true };
		},
		readFile: async (path) => files[path].data,
	};
	const blocks = await promptImageBlocks("@/tmp/a.png @/tmp/big.png @/tmp/missing.png", fs);
	assert.equal(blocks.length, 1);
	assert.deepEqual(blocks[0], {
		type: "image",
		source: { type: "base64", media_type: "image/png", data: Buffer.from("abc").toString("base64") },
	});
});
