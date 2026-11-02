// Writes the Lucide icons the app uses as SVG files, from the lucide-react
// package (apps/native/package.json). The stroke width stays a `{{stroke}}`
// placeholder: views ask for the exact width they draw. The provider marks,
// sonner's toast marks and the diff's expand marks in assets/ were taken
// once from the desktop and are kept as they are.
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
	"triangle-alert", "file", "file-image", "file-headphone", "ban", "shield-alert", "arrow-up", "square", "image", "braces", "paperclip", "shield", "shield-check", "clipboard-list", "zap", "star", "play", "target", "folder-git-2", "pause", "list-plus", "corner-up-right", "arrow-down", "arrow-up-right", "boxes", "git-fork", "box", "link-2", "at-sign", "panel-left-close", "git-compare-arrows", "chevrons-down-up", "chevrons-up-down", "laptop", "list", "list-tree", "undo-2", "minus", "cloud", "arrow-right", "external-link", "refresh-cw", "git-commit-horizontal", "server", "file-exclamation-point", "message-square-warning", "users", "user-round-check", "circle-x", "git-merge", "circle-dot", "rotate-cw", "user-plus", "message-square", "file-terminal", "ban",
	"corner-left-up", "circle-stop", "brain-circuit", "cloud-off", "eye-off", "pencil-line", "user-round", "activity", "timer", "monitor",
	"message-square-text", "messages-square", "badge-check", "lightbulb", "newspaper", "arrow-left-right", "users-round", "git-pull-request-arrow", "smartphone", "coffee", "goal", "cable", "workflow", "app-window", "circle-user-round", "keyboard", "gauge", "coins", "bug", "arrow-left", "download", "folder-search", "sun-medium", "moon", "monitor-cog", "log-in", "command", "key-round", "power", "wifi", "qr-code", "message-square-plus", "reply", "check-check", "message-circle", "grip", "arrow-down-left", "camera", "lock-keyhole", "house",
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

console.log(`wrote ${ICONS.length} icons`);
