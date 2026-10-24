// Writes the Pierre light and dark themes the desktop's diffs use
// (@pierre/theme, the Shiki themes behind "pierre-light" / "pierre-dark").
// Run from the repo root after `yarn install`: node apps/native/scripts/gen-diff-themes.mjs
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const themes = join(here, "../../../node_modules/@pierre/theme/dist");
const out = join(here, "../assets/diff-themes");
mkdirSync(out, { recursive: true });
for (const name of ["pierre-light", "pierre-dark"]) {
	const theme = (await import(pathToFileURL(join(themes, `${name}.mjs`)).href)).default;
	const { colors, tokenColors, type } = theme;
	writeFileSync(join(out, `${name}.json`), JSON.stringify({ name, type, colors, tokenColors }));
}
