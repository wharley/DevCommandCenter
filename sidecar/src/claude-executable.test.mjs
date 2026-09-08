import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmodSync, mkdtempSync, realpathSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { claudeCommand, resolveClaudeExecutable } from "./claude-executable.mjs";

function virtualResolver(env, files, platform = "darwin") {
	return resolveClaudeExecutable({ env, platform, canExecute: (file) => files.includes(file), canonicalize: (file) => file });
}

test("resolves a GUI launch without shell PATH using the native user install", () => {
	assert.equal(virtualResolver({ HOME: "/Users/person", PATH: "/usr/bin" }, ["/Users/person/.local/bin/claude"]), "/Users/person/.local/bin/claude");
});

test("keeps PATH precedence and supports Homebrew when PATH is minimal", () => {
	assert.equal(virtualResolver({ HOME: "/Users/person", PATH: "/custom/bin:/usr/bin" }, ["/custom/bin/claude", "/Users/person/.local/bin/claude"]), "/custom/bin/claude");
	assert.equal(virtualResolver({ PATH: "/usr/bin" }, ["/opt/homebrew/bin/claude"]), "/opt/homebrew/bin/claude");
});

test("missing CLI gives installation/login guidance and never resolves a bundled copy", () => {
	assert.throws(() => virtualResolver({ HOME: "/empty", PATH: "" }, ["/app/Resources/vendor/claude-code/claude", "/app/node_modules/@anthropic-ai/claude-code/bin/claude.exe"]), /Install it.*claude auth login/);
});

test("explicit path wins and a broken override does not silently select another CLI", () => {
	const installed = ["/custom/Claude Code/claude", "/usr/bin/claude"];
	assert.equal(virtualResolver({ DCC_CLAUDE_CODE_BIN_PATH: installed[0] }, installed), installed[0]);
	assert.throws(() => virtualResolver({ DCC_CLAUDE_CODE_BIN_PATH: "/missing", PATH: "/usr/bin" }, installed), /configured Claude Code executable/);
	assert.throws(() => virtualResolver({ DCC_CLAUDE_CODE_BIN_PATH: "claude" }, installed), /absolute path/);
});

test("ignores empty/relative PATH entries instead of searching the project directory", () => {
	assert.throws(() => virtualResolver({ PATH: ":.:node_modules/.bin" }, ["claude", "node_modules/.bin/claude"]), /CLI was not found/);
});

test("Windows npm launchers resolve to native package entrypoints without a shell", () => {
	const dir = "C:\\Users\\Person Name\\AppData\\Roaming\\npm";
	const native = `${dir}\\node_modules\\@anthropic-ai\\claude-code\\bin\\claude.exe`;
	assert.equal(virtualResolver({ Path: `"${dir}"` }, [`${dir}\\claude.cmd`, native], "win32"), native);
});

test("Windows discovers native user installs and older npm JS entrypoints", () => {
	const home = "C:\\Users\\Person";
	const native = `${home}\\.local\\bin\\claude.exe`;
	assert.equal(virtualResolver({ USERPROFILE: home }, [native], "win32"), native);
	const js = "C:\\npm\\node_modules\\@anthropic-ai\\claude-code\\cli.js";
	assert.equal(virtualResolver({ APPDATA: "C:\\" }, ["C:\\npm\\claude.cmd", js], "win32"), js);
	assert.deepEqual(claudeCommand(js, ["auth", "status"]), { command: "node", args: [js, "auth", "status"] });
});

test("a broken npm shim is actionable and cannot be passed to the SDK", () => {
	assert.throws(() => virtualResolver({ PATH: "C:\\npm" }, ["C:\\npm\\claude.cmd"], "win32"), /Repair your Claude Code installation/);
});

test("native paths with spaces remain a single executable argument", () => {
	assert.deepEqual(claudeCommand("/Users/Person Name/.local/bin/claude", ["--version"]), { command: "/Users/Person Name/.local/bin/claude", args: ["--version"] });
});

test("real symlinks resolve to a stable executable and non-executable files are rejected", { skip: process.platform === "win32" }, (t) => {
	const root = mkdtempSync(join(tmpdir(), "dcc-claude-path-"));
	t.after(() => rmSync(root, { recursive: true, force: true }));
	const cli = join(root, "Claude Code");
	const link = join(root, "claude");
	writeFileSync(cli, "#!/bin/sh\nexit 0\n", { mode: 0o755 });
	symlinkSync(cli, link);
	assert.equal(resolveClaudeExecutable({ env: { DCC_CLAUDE_CODE_BIN_PATH: link } }), realpathSync(cli));
	chmodSync(cli, 0o644);
	assert.throws(() => resolveClaudeExecutable({ env: { DCC_CLAUDE_CODE_BIN_PATH: link } }), /cannot be executed/);
	const js = join(root, "cli.js");
	writeFileSync(js, "console.log('fixture')", { mode: 0o644 });
	assert.equal(resolveClaudeExecutable({ env: { DCC_CLAUDE_CODE_BIN_PATH: js } }), realpathSync(js));
});

test("sidecar probes the external CLI and preserves authentication output without starting a model", (t) => {
	const root = mkdtempSync(join(tmpdir(), "dcc-claude-probe-"));
	t.after(() => rmSync(root, { recursive: true, force: true }));
	const cli = join(root, "cli.js");
	writeFileSync(cli, `if (process.argv.includes('--version')) console.log('2.1.999 (Claude Code)'); else if (process.argv.slice(2).join(' ') === 'auth status') { console.log(JSON.stringify({ loggedIn: false })); process.exitCode = 1; } else throw new Error('unexpected command');`, { mode: 0o755 });
	const entry = fileURLToPath(new URL("./index.mjs", import.meta.url));
	const env = { ...process.env, DCC_CLAUDE_CODE_BIN_PATH: cli, PATH: `${dirname(process.execPath)}${process.platform === "win32" ? ";" : ":"}${process.env.PATH ?? ""}` };
	const run = (args, variables = env) => spawnSync(process.execPath, [entry, ...args], { env: variables, encoding: "utf8", timeout: 15_000 });
	const resolved = run(["--resolve-cli"]);
	assert.equal(resolved.status, 0, resolved.stderr);
	assert.deepEqual(JSON.parse(resolved.stdout), { path: realpathSync(cli), version: "2.1.999 (Claude Code)" });
	const auth = run(["--auth-status"]);
	assert.equal(auth.status, 1);
	assert.equal(JSON.parse(auth.stdout).loggedIn, false);
	const missingEnv = { ...env, DCC_CLAUDE_CODE_BIN_PATH: join(root, "missing") };
	const missing = run(["--resolve-cli"], missingEnv);
	assert.equal(missing.status, 1);
	assert.match(missing.stderr, /configured Claude Code executable/);
	assert.equal(run(["--version"], missingEnv).status, 0, "the DCC helper version must not need a provider installation");
	writeFileSync(cli, "console.log('not a version')");
	const broken = run(["--resolve-cli"]);
	assert.equal(broken.status, 1);
	assert.match(broken.stderr, /Update or repair Claude Code/);
	writeFileSync(cli, "if (process.argv.includes('--version')) console.log('2.0.76 (Claude Code)'); else throw new Error('auth must not be sent as a prompt');");
	const legacyAuth = run(["--auth-status"]);
	assert.equal(legacyAuth.status, 1);
	assert.match(legacyAuth.stderr, /Update Claude Code/);
	assert.match(legacyAuth.stderr, /2\.0\.76/);
	assert.ok(legacyAuth.stderr.includes(realpathSync(cli)));
	assert.doesNotMatch(legacyAuth.stderr, /auth must not be sent/);
});

test("authentication uses the documented version boundary without parsing help or prompting old CLIs", (t) => {
	const root = mkdtempSync(join(tmpdir(), "dcc-claude-auth-"));
	t.after(() => rmSync(root, { recursive: true, force: true }));
	const cli = join(root, "cli.js");
	writeFileSync(cli, `
		if (process.argv.includes('--version')) {
			console.log(process.env.DCC_TEST_VERSION);
			process.exitCode = Number(process.env.DCC_TEST_VERSION_EXIT || 0);
		} else if (process.env.DCC_TEST_AUTH_ALLOWED === '1' && process.argv.slice(2).join(' ') === 'auth status') {
			console.log(JSON.stringify({ loggedIn: true }));
		} else throw new Error('unexpected command: must not parse help or prompt a model');
	`);
	const entry = fileURLToPath(new URL("./index.mjs", import.meta.url));
	for (const [version, supported, versionExit = 0] of [
		["1.9.999", false], ["2.0.76", false], ["2.1.40", false],
		["2.1.41", true], ["2.1.259", true], ["2.2.0", true], ["3.0.0", true],
		["unknown", false], ["error from runtime 22.21.1", false],
		["2.1.259", false, 7],
	]) {
		const result = spawnSync(process.execPath, [entry, "--auth-status"], {
			env: { ...process.env, DCC_CLAUDE_CODE_BIN_PATH: cli,
				PATH: `${dirname(process.execPath)}${process.platform === "win32" ? ";" : ":"}${process.env.PATH ?? ""}`,
				DCC_TEST_VERSION: version, DCC_TEST_VERSION_EXIT: String(versionExit),
				DCC_TEST_AUTH_ALLOWED: supported ? "1" : "0" },
			encoding: "utf8", timeout: 15_000,
		});
		assert.equal(result.status, supported ? 0 : 1, `${version}: ${result.stderr}`);
		assert.doesNotMatch(result.stderr, /unexpected command/);
		if (supported) assert.equal(JSON.parse(result.stdout).loggedIn, true);
		else assert.ok(result.stderr.includes(realpathSync(cli)), result.stderr);
		if (versionExit) {
			assert.match(result.stderr, /exit 7/);
			assert.doesNotMatch(result.stderr, /does not support authentication/);
		}
	}
});
