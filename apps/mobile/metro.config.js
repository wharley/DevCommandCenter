// Metro inside the yarn workspace. Expo's default config already watches the
// monorepo root and resolves hoisted packages; on top of it:
// - yarn 1 leaves one copy of react-native hoisted and another under this app
//   (same version). Pin `react` and `react-native` to the copy autolinking
//   builds against, or the bundle ends up with two renderers. Paseo pins
//   react the same way (packages/app/metro.config.cjs).
// - Keep Metro out of build outputs it would otherwise crawl.
const { getDefaultConfig } = require("expo/metro-config");
const path = require("node:path");

const projectRoot = __dirname;
const workspaceRoot = path.resolve(projectRoot, "../..");
const config = getDefaultConfig(projectRoot);

const pinned = Object.fromEntries(
	["react", "react-native"].map((name) => [
		name,
		path.dirname(require.resolve(`${name}/package.json`, { paths: [projectRoot] })),
	]),
);

const escape = (value) => value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
config.resolver.blockList = [
	...[].concat(config.resolver.blockList ?? []),
	new RegExp(`^${escape(workspaceRoot)}/(target|src-tauri|crates|apps/desktop|apps/relay)/.*`),
	new RegExp(`^${escape(projectRoot)}/(ios|android)/.*`),
];

const defaultResolve = config.resolver.resolveRequest;
config.resolver.resolveRequest = (context, moduleName, platform) => {
	const [scopeOrName] = moduleName.split("/");
	const root = pinned[scopeOrName];
	if (root) {
		const target = path.join(root, moduleName.slice(scopeOrName.length));
		return (defaultResolve ?? context.resolveRequest)(context, target, platform);
	}
	return (defaultResolve ?? context.resolveRequest)(context, moduleName, platform);
};

module.exports = config;
