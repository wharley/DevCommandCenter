import { accessSync, constants, realpathSync, statSync } from "node:fs";
import path from "node:path";

export const CLAUDE_INSTALL_HELP =
	"Claude Code CLI was not found. Install it from https://code.claude.com/docs/en/setup, then run `claude auth login` and check the provider again.";

function isExecutable(file, platform) {
	try {
		if (!statSync(file).isFile()) return false;
		const mode = platform === "win32" ? constants.F_OK
			: /\.m?js$/i.test(realpathSync(file)) ? constants.R_OK : constants.X_OK;
		accessSync(file, mode);
		return true;
	} catch {
		return false;
	}
}

// The SDK needs an actual file, not a shell alias or a Windows npm launcher.
// No lookup through our node_modules: the CLI belongs to the user's installation.
export function resolveClaudeExecutable({
	env = process.env,
	platform = process.platform,
	canExecute = (file) => isExecutable(file, platform),
	canonicalize = realpathSync,
} = {}) {
	const paths = platform === "win32" ? path.win32 : path.posix;
	const home = env.HOME || env.USERPROFILE;
	const searchPath = env.PATH ?? env.Path ?? "";
	const directories = searchPath.split(paths.delimiter)
		.map((entry) => entry.replace(/^"(.*)"$/, "$1"))
		.filter((entry) => paths.isAbsolute(entry));
	if (home) {
		directories.push(paths.join(home, ".local", "bin"), paths.join(home, ".claude", "local"));
	}
	if (platform === "win32") {
		if (env.APPDATA) directories.push(paths.join(env.APPDATA, "npm"));
	} else {
		directories.push("/opt/homebrew/bin", "/usr/local/bin", "/usr/bin");
	}

	const override = env.DCC_CLAUDE_CODE_BIN_PATH?.trim();
	if (override && !paths.isAbsolute(override)) {
		throw new Error("DCC_CLAUDE_CODE_BIN_PATH must be an absolute path to the installed Claude Code CLI.");
	}
	const names = platform === "win32" ? ["claude.exe", "claude.cmd", "claude.bat", "claude.ps1"] : ["claude"];
	const candidates = override ? [override] : [...new Set(directories)].flatMap((dir) => names.map((name) => paths.join(dir, name)));
	for (const candidate of candidates) {
		if (!canExecute(candidate)) continue;
		const resolved = canonicalize(candidate);
		if (platform === "win32" && /\.(cmd|bat|ps1)$/i.test(resolved)) {
			const packageDir = paths.join(paths.dirname(resolved), "node_modules", "@anthropic-ai", "claude-code");
			for (const entry of [paths.join(packageDir, "bin", "claude.exe"), paths.join(packageDir, "cli.js")]) {
				if (canExecute(entry)) return canonicalize(entry);
			}
			throw new Error("The Claude Code launcher has no executable beside it. Repair your Claude Code installation, then check the provider again.");
		}
		return resolved;
	}
	throw new Error(override
		? "The configured Claude Code executable is missing or cannot be executed. Fix DCC_CLAUDE_CODE_BIN_PATH, then check the provider again."
		: CLAUDE_INSTALL_HELP);
}

export function claudeCommand(executable, args) {
	// Older npm installs are JS entrypoints. Their Node installation is required;
	// the compiled DCC sidecar must never be used as a general-purpose JS runtime.
	return /\.m?js$/i.test(executable)
		? { command: "node", args: [executable, ...args] }
		: { command: executable, args };
}
