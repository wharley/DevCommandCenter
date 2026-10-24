// Writes the Lucide icons the native app uses as SVG files, from the same
// lucide-react package the desktop renders. The stroke width stays a
// `{{stroke}}` placeholder: views ask for the exact width the desktop uses.
// Run from the repo root after `yarn install`: node apps/native/scripts/gen-icons.mjs
import { mkdirSync, readFileSync, writeFileSync, copyFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ICONS = [
	"alarm-clock", "alarm-clock-off", "orbit", "x", "check", "bot", "brush-cleaning", "chart-column",
	"chevron-down", "chevron-right", "chevron-up", "circle-check", "circle-check-big", "circle-pause",
	"circle-question-mark", "clock-3", "code-xml", "cpu", "database", "earth",
	"ellipsis", "folder", "folder-open", "folder-plus", "folder-tree",
	"git-branch", "git-pull-request", "layers", "loader-circle",
	"message-square-more", "package", "panel-left", "panel-right", "pencil",
	"pin", "pin-off", "plus", "rocket", "rotate-ccw", "search", "settings-2",
	"shield-check", "sparkles", "square-terminal", "sticky-note", "trash-2",
	"wrench", "terminal", "file-pen-line", "file-text", "globe", "list-todo", "plug", "brain",
	"circle-alert", "circle-dashed", "circle", "corner-down-right", "hourglass", "copy", "file-code-corner",
	"triangle-alert", "file", "file-image", "file-headphone", "ban", "shield-alert", "arrow-up", "square", "image", "braces", "paperclip", "shield", "shield-check", "clipboard-list", "zap", "star", "play", "target", "folder-git-2", "pause", "list-plus", "corner-up-right", "arrow-down", "arrow-up-right", "boxes", "git-fork", "box", "link-2", "at-sign", "panel-left-close", "git-compare-arrows", "chevrons-down-up", "chevrons-up-down", "laptop", "list", "list-tree", "undo-2", "minus", "cloud", "arrow-right", "external-link", "refresh-cw",
];

/** Icons the app also draws filled. */
const FILLED = ["star", "zap", "play"];

const here = dirname(fileURLToPath(import.meta.url));
const lucide = join(here, "../../../node_modules/lucide-react");
const out = join(here, "../assets/icons");
mkdirSync(out, { recursive: true });

const escape = (value) => String(value).replace(/&/g, "&amp;").replace(/"/g, "&quot;");

for (const name of ICONS) {
	const source = readFileSync(join(lucide, `dist/esm/icons/${name}.mjs`), "utf8");
	const literal = source.match(/const __iconNode = (\[[\s\S]*?\]);\n/)?.[1];
	if (!literal) throw new Error(`no icon node in ${name}`);
	const nodes = Function(`return ${literal}`)();
	const children = nodes
		.map(([tag, attrs]) => {
			const pairs = Object.entries(attrs)
				.filter(([key]) => key !== "key")
				.map(([key, value]) => `${key}="${escape(value)}"`)
				.join(" ");
			return `<${tag} ${pairs}/>`;
		})
		.join("");
	const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="{{stroke}}" stroke-linecap="round" stroke-linejoin="round">${children}</svg>\n`;
	writeFileSync(join(out, `${name}.svg`), svg);
	// `fill="currentColor"` (a starred ⭐, Resposta direta on): a filled twin.
	if (FILLED.includes(name)) {
		writeFileSync(join(out, `${name}-filled.svg`), svg.replace('fill="none"', 'fill="#000"'));
	}
}
copyFileSync(join(lucide, "LICENSE"), join(out, "LICENSE-lucide"));

// Provider brand marks, from the desktop's own ProviderIcon sources.
const desktop = join(here, "../../desktop/src");
const providerSource = readFileSync(join(desktop, "features/providers/provider-icons.tsx"), "utf8");
const cursorSource = readFileSync(join(desktop, "components/editor-brand-icons.tsx"), "utf8");
const markPaths = (source, fn) => {
	const body = source.slice(source.indexOf(`function ${fn}`));
	const end = body.indexOf("\n}\n");
	const chunk = body.slice(0, end);
	const viewBox = chunk.match(/viewBox="([^"]+)"/)[1];
	const paths = [...chunk.matchAll(/\sd="([^"]+)"/g)].map((match) => match[1]);
	return { viewBox, paths };
};
const providers = join(here, "../assets/providers");
mkdirSync(providers, { recursive: true });
const mono = (name, { viewBox, paths }) =>
	writeFileSync(
		join(providers, `${name}.svg`),
		`<svg xmlns="http://www.w3.org/2000/svg" viewBox="${viewBox}" fill="#000">${paths.map((d) => `<path d="${d}"/>`).join("")}</svg>\n`,
	);
mono("codex", markPaths(providerSource, "CodexMark"));
mono("droid", markPaths(providerSource, "DroidMark"));
mono("cursor", markPaths(cursorSource, "CursorEditorIcon"));
const claude = markPaths(providerSource, "ClaudeMark");
writeFileSync(
	join(providers, "claude.svg"),
	`<svg xmlns="http://www.w3.org/2000/svg" viewBox="${claude.viewBox}" fill="none">${claude.paths.map((d) => `<path fill="#D97757" d="${d}"/>`).join("")}</svg>\n`,
);
const gemini = markPaths(providerSource, "GeminiMark");
const stops = [...providerSource.slice(providerSource.indexOf("function GeminiMark")).matchAll(/<stop(?: offset="([^"]+)")? stopColor="([^"]+)"/g)]
	.slice(0, 3)
	.map(([, offset, color]) => `<stop${offset ? ` offset="${offset}"` : ""} stop-color="${color}"/>`)
	.join("");
writeFileSync(
	join(providers, "gemini.svg"),
	`<svg xmlns="http://www.w3.org/2000/svg" viewBox="${gemini.viewBox}" fill="none"><defs><linearGradient id="g" x1="32" y1="32" x2="264" y2="266" gradientUnits="userSpaceOnUse">${stops}</linearGradient></defs>${gemini.paths.map((d) => `<path fill="url(#g)" d="${d}"/>`).join("")}</svg>\n`,
);
console.log("wrote provider marks");

// The marks sonner draws in its toasts (filled, so no stroke).
const sonnerSource = readFileSync(join(here, "../../../node_modules/sonner/dist/index.mjs"), "utf8");
const sonnerIcon = (name) => {
	const chunk = sonnerSource.slice(sonnerSource.indexOf(`const ${name} =`));
	const viewBox = chunk.match(/viewBox: "([^"]+)"/)[1];
	const d = chunk.match(/d: "([^"]+)"/)[1];
	return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${viewBox}" fill="#000"><path fill-rule="evenodd" clip-rule="evenodd" d="${d}"/></svg>\n`;
};
writeFileSync(join(out, "sonner-error.svg"), sonnerIcon("ErrorIcon"));
writeFileSync(join(out, "sonner-success.svg"), sonnerIcon("SuccessIcon"));
writeFileSync(join(out, "sonner-info.svg"), sonnerIcon("InfoIcon"));
writeFileSync(join(out, "sonner-warning.svg"), sonnerIcon("WarningIcon"));
console.log(`wrote ${ICONS.length} icons`);

// The diff's own expand marks (@pierre/diffs' sprite), filled.
const sprite = readFileSync(join(here, "../../../node_modules/@pierre/diffs/dist/sprite.js"), "utf8");
for (const [symbol, file] of [["diffs-icon-expand", "diffs-expand"], ["diffs-icon-expand-all", "diffs-expand-all"]]) {
	const start = sprite.indexOf(`<symbol id="${symbol}"`);
	const body = sprite.slice(sprite.indexOf(">", start) + 1, sprite.indexOf("</symbol>", start)).trim();
	writeFileSync(join(out, `${file}.svg`), `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="#000">${body}</svg>`);
}
