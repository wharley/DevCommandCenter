// Writes the Material file and folder icons the code column shows, from the
// same file-extension-icon-js package the desktop renders, plus the name and
// extension tables that pick them (getMaterialFileIcon / getMaterialFolderIcon).
// Run from the repo root after `yarn install`: node apps/native/scripts/gen-file-icons.mjs
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);
const dist = join(here, "../../../node_modules/file-extension-icon-js/dist");
const icons = require(join(dist, "icons/index.js")).default;
const names = require(join(dist, "gen/materialFileIconsName.js"));
const folders = require(join(dist, "gen/materialFolderIconsName.js"));

const out = join(here, "../assets/file-icons");
rmSync(out, { recursive: true, force: true });
const write = (folder, table) => {
	mkdirSync(join(out, folder), { recursive: true });
	for (const [name, svg] of Object.entries(table)) {
		writeFileSync(join(out, folder, `${name}.svg`), svg);
	}
};
write("file", Object.assign({}, ...icons.materialFileIcons));
write("folder", icons.materialFolderIcons);
write("folder-open", icons.materialFolderIconsOpen);
writeFileSync(
	join(out, "names.json"),
	JSON.stringify({
		files: names.materialFileNamesToIcons,
		extensions: names.materialFileExtensionsToIcons,
		folders: folders.materialFolderNamesToIcons,
	}),
);
