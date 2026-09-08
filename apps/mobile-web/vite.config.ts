import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";
import fs from "node:fs";
import { createHash } from "node:crypto";

// The SPA is served by the desktop backend (dccd-http) under the /m/ prefix.
// We build with `base: "/m/"` so asset URLs in the produced index.html are
// absolute under that path, and we proxy /api + /auth + /health in dev so the
// dev server can talk to a locally-running dccd-http on :9876.
export default defineConfig({
	base: "/m/",
	plugins: [
		react(),
		{
			name: "dcc-offline-shell",
			apply: "build",
			closeBundle() {
				const dist = path.resolve(import.meta.dirname, "dist");
				const files = fs
					.readdirSync(dist, { recursive: true })
					.map(String)
					.filter(
						(file) =>
							file !== "sw.js" && fs.statSync(path.join(dist, file)).isFile(),
					)
					.sort();
				const digest = createHash("sha256");
				for (const file of files)
					digest.update(file).update(fs.readFileSync(path.join(dist, file)));
				const template = fs.readFileSync(
					path.resolve(import.meta.dirname, "public/sw.js"),
					"utf8",
				);
				fs.writeFileSync(
					path.join(dist, "sw.js"),
					template
						.replace(
							"/* DCC_PRECACHE */ []",
							JSON.stringify(files.map((file) => `/m/${file}`)),
						)
						.replace("/* DCC_VERSION */", digest.digest("hex").slice(0, 16)),
				);
			},
		},
	],
	resolve: {
		alias: {
			"@": path.resolve(__dirname, "src"),
		},
	},
	server: {
		port: 5174,
		proxy: {
			"/api": "http://127.0.0.1:9876",
			"/auth": "http://127.0.0.1:9876",
			"/health": "http://127.0.0.1:9876",
			"/rpc": "http://127.0.0.1:9876",
		},
	},
	build: {
		outDir: "dist",
		emptyOutDir: true,
	},
});
