/**
 * mcp-remote 0.1.38 proxies the initialize response but does not pass the
 * negotiated protocol version back to its Streamable HTTP transport. Strict
 * MCP servers then reject every subsequent request because the required
 * MCP-Protocol-Version header is absent.
 *
 * Keep this narrow, pinned patch until upstream carries equivalent behavior.
 */
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const packageDir = path.join(scriptDir, "..", "node_modules", "mcp-remote");
const packageJsonPath = path.join(packageDir, "package.json");
const protocolMarker = "/* dcc: propagate negotiated MCP protocol version */";
const stateDirectoryMarker =
	"/* dcc: use the exact private OAuth state directory when provided */";

if (!fs.existsSync(packageJsonPath)) {
	console.warn("[patch-mcp-remote] skip: node_modules/mcp-remote missing");
	process.exit(0);
}

const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, "utf8"));
if (packageJson.version !== "0.1.38") {
	throw new Error(
		`[patch-mcp-remote] unsupported version ${packageJson.version}; expected 0.1.38`,
	);
}

const distDir = path.join(packageDir, "dist");
const candidates = fs
	.readdirSync(distDir)
	.filter((name) => /^chunk-[A-Za-z0-9]+\.js$/.test(name))
	.map((name) => path.join(distDir, name))
	.filter((filePath) =>
		fs.readFileSync(filePath, "utf8").includes("function mcpProxy({"),
	);

if (candidates.length !== 1) {
	throw new Error(
		`[patch-mcp-remote] expected one proxy chunk, found ${candidates.length}`,
	);
}

const target = candidates[0];
let source = fs.readFileSync(target, "utf8");
let changed = false;

if (source.includes(protocolMarker)) {
	console.log("[patch-mcp-remote] protocol version propagation already applied");
} else {
	const stateBefore = `  let transportToClientClosed = false;
  let transportToServerClosed = false;`;
	const stateAfter = `  ${protocolMarker}
  let initializeRequestId = null;
  let transportToClientClosed = false;
  let transportToServerClosed = false;`;
	const requestBefore = `    if (message.method === "initialize") {
      const { clientInfo } = message.params;`;
	const requestAfter = `    if (message.method === "initialize") {
      initializeRequestId = message.id;
      const { clientInfo } = message.params;`;
	const responseBefore = `  transportToServer.onmessage = (_message) => {
    const message = messageTransformer.interceptResponse(_message);
    log("[Remote\\u2192Local]", message.method || message.id);`;
	const responseAfter = `  transportToServer.onmessage = (_message) => {
    const message = messageTransformer.interceptResponse(_message);
    if (initializeRequestId !== null && message.id === initializeRequestId && typeof message.result?.protocolVersion === "string") {
      transportToServer.setProtocolVersion(message.result.protocolVersion);
      initializeRequestId = null;
    }
    log("[Remote\\u2192Local]", message.method || message.id);`;

	for (const [before, after, label] of [
		[stateBefore, stateAfter, "state"],
		[requestBefore, requestAfter, "initialize request"],
		[responseBefore, responseAfter, "initialize response"],
	]) {
		if (!source.includes(before)) {
			throw new Error(`[patch-mcp-remote] ${label} snippet not found`);
		}
		source = source.replace(before, after);
	}
	changed = true;
}

if (source.includes(stateDirectoryMarker)) {
	console.log("[patch-mcp-remote] private OAuth state directory already applied");
} else {
	const directoryBefore = `function getConfigDir() {
  const baseConfigDir = process.env.MCP_REMOTE_CONFIG_DIR || path.join(os.homedir(), ".mcp-auth");
  return path.join(baseConfigDir, \`mcp-remote-\${version2}\`);
}`;
	const directoryAfter = `function getConfigDir() {
  ${stateDirectoryMarker}
  const dccStateDirectory = process.env.DCC_MCP_REMOTE_STATE_DIR;
  if (dccStateDirectory) return dccStateDirectory;
  const baseConfigDir = process.env.MCP_REMOTE_CONFIG_DIR || path.join(os.homedir(), ".mcp-auth");
  return path.join(baseConfigDir, \`mcp-remote-\${version2}\`);
}`;
	if (!source.includes(directoryBefore)) {
		throw new Error(
			"[patch-mcp-remote] OAuth state directory snippet not found",
		);
	}
	source = source.replace(directoryBefore, directoryAfter);
	changed = true;
}

if (changed) {
	fs.writeFileSync(target, source);
	console.log("[patch-mcp-remote] DCC compatibility patches applied");
}
