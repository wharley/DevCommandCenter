import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

const dirname = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig({
	publicDir: path.resolve(dirname, "../../public"),
	test: {
		environment: "jsdom",
		globals: true,
	},
	resolve: {
		alias: {
			"@": path.resolve(dirname, "./src"),
		},
	},
});
