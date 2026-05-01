import { defineConfig } from "vitest/config";

export const vitestBaseConfig = defineConfig({
	test: {
		environment: "jsdom",
		globals: true,
	},
});

export default vitestBaseConfig;
