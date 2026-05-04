import { spawnSync } from "node:child_process";

function run(command, args) {
	const result = spawnSync(command, args, {
		stdio: "inherit",
		env: process.env,
	});

	if (result.error) {
		throw result.error;
	}
	if (result.status !== 0) {
		process.exit(result.status ?? 1);
	}
}

run("node", ["scripts/stage-vendor.mjs"]);

const probe = spawnSync("bun", ["--version"], {
	stdio: "ignore",
	env: process.env,
});

if (probe.status !== 0) {
	console.error(
		"[sidecar] Bun is required only to compile the production Claude sidecar binary. Runtime in dev now uses Node.",
	);
	process.exit(probe.status ?? 1);
}

run("bun", ["build", "--compile", "src/index.mjs", "--outfile", "dist/dcc-claude-sidecar"]);
