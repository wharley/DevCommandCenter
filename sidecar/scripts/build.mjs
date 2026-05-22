import { spawnSync } from "node:child_process";

function resolveCommand(command) {
	if (process.platform !== "win32") {
		return command;
	}

	if (command === "yarn") {
		return "yarn.cmd";
	}

	return command;
}

function run(command, args) {
	const resolvedCommand = resolveCommand(command);
	const result = spawnSync(resolvedCommand, args, {
		stdio: "inherit",
		env: process.env,
	});

	if (result.error) {
		console.error(`[sidecar] failed to start ${resolvedCommand}:`, result.error);
		process.exit(1);
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
