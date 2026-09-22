import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const installer = fileURLToPath(new URL("./install.mjs", import.meta.url));

function fixture(t, cachedSource, installedVersion = "0.2.2", cargoExit = 0) {
	const directory = mkdtempSync(join(tmpdir(), "dcc-oj-test-"));
	t.after(() => rmSync(directory, { recursive: true, force: true }));
	const root = join(directory, "install root");
	const tools = join(directory, "tools");
	mkdirSync(join(root, "bin"), { recursive: true });
	mkdirSync(tools);
	const binary = join(root, "bin", "oj");
	if (cachedSource !== undefined) {
		writeFileSync(binary, `#!/usr/bin/env node\n${cachedSource}\n`, { mode: 0o755 });
	}
	const log = join(directory, "cargo-args.json");
	const githubPath = join(directory, "github-path");
	writeFileSync(join(tools, "cargo"), `#!/usr/bin/env node
const fs = require('node:fs');
const path = require('node:path');
const args = process.argv.slice(2);
fs.writeFileSync(${JSON.stringify(log)}, JSON.stringify(args));
if (${cargoExit}) process.exit(${cargoExit});
const root = args[args.indexOf('--root') + 1];
fs.mkdirSync(path.join(root, 'bin'), { recursive: true });
fs.writeFileSync(path.join(root, 'bin', 'oj'),
  '#!/usr/bin/env node\\nconsole.log("oj ${installedVersion}");\\n', { mode: 0o755 });
`, { mode: 0o755 });
	const run = () => spawnSync(process.execPath, [installer], {
		encoding: "utf8",
		env: {
			...process.env,
			PATH: `${tools}${delimiter}${process.env.PATH}`,
			DCC_OJ_ROOT: root,
			DCC_OJ_VERSION: "0.2.2",
			GITHUB_PATH: githubPath,
		},
	});
	return { run, root, log, githubPath };
}

test("a valid cached binary skips Cargo and exposes the isolated installation", (t) => {
	const f = fixture(t, 'console.log("oj 0.2.2");');
	const result = f.run();
	assert.equal(result.status, 0, result.stderr);
	assert.equal(existsSync(f.log), false);
	assert.equal(readFileSync(f.githubPath, "utf8"), `${join(f.root, "bin")}\n`);
});

for (const [name, source] of [
	["missing", undefined],
	["wrong version", 'console.log("oj 0.2.1");'],
	["broken", "process.exit(1);"],
]) {
	test(`${name} cache falls back to a pinned, locked Cargo install`, (t) => {
		const f = fixture(t, source);
		const result = f.run();
		assert.equal(result.status, 0, result.stderr);
		assert.deepEqual(JSON.parse(readFileSync(f.log, "utf8")), [
			"install", "oj", "--version", "0.2.2", "--locked", "--root", f.root, "--force",
		]);
		assert.equal(readFileSync(f.githubPath, "utf8"), `${join(f.root, "bin")}\n`);
		// Once installed, a subsequent run can reuse the executable.
		assert.match(f.run().stdout, /Reusing cached OJ/);
	});
}

test("a failed Cargo install fails setup without adding an invalid binary to PATH", (t) => {
	const f = fixture(t, undefined, "0.2.2", 42);
	assert.equal(f.run().status, 42);
	assert.equal(existsSync(f.githubPath), false);
});

test("an incorrect version after installation also fails setup", (t) => {
	const f = fixture(t, undefined, "0.2.3");
	const result = f.run();
	assert.notEqual(result.status, 0);
	assert.match(result.stderr, /failed the version\/executable check/);
	assert.equal(existsSync(f.githubPath), false);
});
