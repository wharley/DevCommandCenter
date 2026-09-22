import { spawnSync } from "node:child_process";
import { appendFileSync } from "node:fs";
import { isAbsolute, join } from "node:path";

const version = process.env.DCC_OJ_VERSION;
const root = process.env.DCC_OJ_ROOT;
if (!/^\d+\.\d+\.\d+$/.test(version ?? "") || !root || !isAbsolute(root)) {
	throw new Error("An exact DCC_OJ_VERSION and absolute DCC_OJ_ROOT are required.");
}
if (!process.env.GITHUB_PATH) {
	throw new Error("GITHUB_PATH is required to expose the OJ installation.");
}

const binary = join(root, "bin", "oj");
function validInstallation() {
	const result = spawnSync(binary, ["--version"], {
		encoding: "utf8",
		timeout: 10_000,
		killSignal: "SIGKILL",
	});
	return !result.error && result.status === 0 && result.stdout.trim() === `oj ${version}`;
}

if (validInstallation()) {
	console.log(`Reusing cached OJ ${version}.`);
} else {
	console.log(`OJ ${version} is missing or unusable; compiling with Cargo.`);
	const result = spawnSync(
		"cargo",
		["install", "oj", "--version", version, "--locked", "--root", root, "--force"],
		{ stdio: "inherit" },
	);
	if (result.error || result.status !== 0) {
		console.error(result.error?.message ?? "OJ installation failed.");
		process.exit(result.status || 1);
	}
	if (!validInstallation()) {
		throw new Error(`Installed OJ failed the version/executable check for ${version}.`);
	}
}

appendFileSync(process.env.GITHUB_PATH, `${join(root, "bin")}\n`);
