import path from "node:path";
import { fileURLToPath } from "node:url";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

const dirname = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig({
	plugins: [react()],
	test: {
		environment: "jsdom",
		globals: true,
	},
	resolve: {
		alias: {
			"@": path.resolve(dirname, "./src"),
		},
	},
	build: {
		outDir: "dist",
		emptyOutDir: true,
	},
	server: {
		port: 1420,
		strictPort: true,
	},
	optimizeDeps: {
		include: ["react", "react-dom", "@tanstack/react-query"],
	},
});
