// Builds the store images from HTML: the Play feature graphic and the
// captioned screenshots for both stores, from the raw prints in
// app-store/raw/. The pixel sprites come straight from @dcc/mascots, so the
// robots and the fauna are the same drawings the app shows.
//
//   PATH="$HOME/.nvm/versions/node/v22.21.1/bin:$PATH" node apps/mobile/store-assets/source/build.mjs
//
// Needs Node 22 (loads the mascots' TypeScript with type stripping) and macOS
// (render.swift snapshots each page with WebKit into an opaque PNG).

import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { agentMascotPaths, projectMascotPaths } from "../../../../packages/mascots/src/index.ts";

const here = dirname(fileURLToPath(import.meta.url));
const root = dirname(here); // store-assets/
const work = join(root, ".build");

const C = {
	bg: "#0a0b0d",
	panel: "#111419",
	border: "#232834",
	fg: "#e9edf2",
	mute: "#7c8694",
	faint: "#565f6b",
	accent: "#34e2a0",
	wait: "#f5b13d",
};

/** `+` pixels: the sprite color mixed halfway to white, as in the app. */
function lighter(hex) {
	const n = Number.parseInt(hex.slice(1, 7), 16);
	const mix = (c) => Math.round(c + (255 - c) / 2);
	return `#${((mix((n >> 16) & 255) << 16) | (mix((n >> 8) & 255) << 8) | mix(n & 255)).toString(16).padStart(6, "0")}`;
}

function sprite(frame, tint, size) {
	return `<svg width="${size}" height="${size}" viewBox="0 0 10 10" shape-rendering="crispEdges"><path d="${frame.body}" fill="${tint}"/>${frame.accent ? `<path d="${frame.accent}" fill="${lighter(tint)}"/>` : ""}</svg>`;
}

const AGENT_TILES = {
	claude: { base: "#D97757", ink: "#E8936F" },
	codex: { base: "#3B7BF6", ink: "#7AA5FA" },
	gemini: { base: "#8B5CF6", ink: "#C4B5FD" },
};

const PROJECT_TILES = {
	emerald: { base: "#10b981", ink: "#6ee7b7" },
	sky: { base: "#0ea5e9", ink: "#7dd3fc" },
	orange: { base: "#f97316", ink: "#fdba74" },
	violet: { base: "#8b5cf6", ink: "#c4b5fd" },
	rose: { base: "#f43f5e", ink: "#fda4af" },
	lime: { base: "#84cc16", ink: "#bef264" },
};

function tile(inner, base, size, radius) {
	return `<div class="tile" style="width:${size}px;height:${size}px;border-radius:${radius}px;border-color:${base}4d;background:${base}1f">${inner}</div>`;
}

function robot(key, state, size, frame = "rest") {
	const waiting = state === "waiting";
	const t = AGENT_TILES[key];
	const ink = waiting ? C.wait : t.ink;
	const base = waiting ? C.wait : t.base;
	return tile(sprite(agentMascotPaths(key, state)[frame], ink, Math.round((size * 0.8) / 10) * 10), base, size, Math.round(size / 4));
}

function bicho(id, colorId, size, frame = "rest") {
	const t = PROJECT_TILES[colorId];
	return tile(sprite(projectMascotPaths(id)[frame], t.ink, Math.round((size * 0.6) / 10) * 10), t.base, size, Math.round(size / 3.5));
}

const BASE_CSS = `
* { box-sizing: border-box; margin: 0; padding: 0; }
html, body { background: ${C.bg}; color: ${C.fg}; -webkit-font-smoothing: antialiased; }
body { font-family: -apple-system, "SF Pro Display", "Helvetica Neue", sans-serif; overflow: hidden; }
.mono { font-family: Menlo, ui-monospace, monospace; }
.tile { display: flex; align-items: center; justify-content: center; border: 2px solid; flex: none; }
.tile svg { display: block; }
.grid {
	position: absolute; inset: 0;
	background-image:
		linear-gradient(${C.border}55 1px, transparent 1px),
		linear-gradient(90deg, ${C.border}55 1px, transparent 1px);
	background-size: 48px 48px;
	-webkit-mask-image: radial-gradient(ellipse at 70% 40%, #000 0%, transparent 75%);
}
`;

function page(title, css, body) {
	return `<!doctype html><html lang="pt-BR"><head><meta charset="utf-8"><title>${title}</title><style>${BASE_CSS}${css}</style></head><body>${body}</body></html>`;
}

// ---------------------------------------------------------------------------
// Google Play feature graphic, 1024×500. Everything that matters stays inside
// a generous safe area: the Play crops and overlays the edges.
// ---------------------------------------------------------------------------

function featureGraphic() {
	const css = `
body { width: 1024px; height: 500px; position: relative; }
.glow { position: absolute; width: 760px; height: 760px; left: 470px; top: -150px; border-radius: 50%;
	background: radial-gradient(circle, ${C.accent}24 0%, transparent 62%); }
.left { position: absolute; left: 84px; top: 0; bottom: 0; width: 470px; display: flex; flex-direction: column; justify-content: center; }
.kicker { display: flex; align-items: center; gap: 12px; font-size: 17px; letter-spacing: 3px; text-transform: uppercase; color: ${C.mute}; }
.dot { width: 13px; height: 13px; border-radius: 50%; background: ${C.accent}; box-shadow: 0 0 14px ${C.accent}; }
.word { font-size: 150px; font-weight: 700; letter-spacing: 14px; line-height: 1; margin: 12px 0 18px -6px; color: ${C.fg}; }
.word b { color: ${C.accent}; font-weight: 700; }
.tag { font-size: 38px; font-weight: 700; line-height: 1.12; letter-spacing: -0.5px; }
.tag span { color: ${C.accent}; }
.right { position: absolute; left: 590px; top: 78px; width: 360px; display: flex; flex-direction: column; gap: 16px; }
.card { display: flex; align-items: center; gap: 16px; padding: 16px 18px; border-radius: 18px; border: 2px solid ${C.border}; background: ${C.panel}; }
.card.wait { border-color: ${C.wait}66; background: #f5b13d10; }
.card .t { font-size: 22px; font-weight: 650; }
.pill { margin-top: 6px; display: inline-flex; align-items: center; gap: 8px; font-size: 17px; letter-spacing: 1.5px; text-transform: uppercase; }
.pill i { width: 9px; height: 9px; border-radius: 50%; display: block; }
.fauna { display: flex; gap: 14px; margin-top: 6px; padding-left: 4px; }
`;
	const body = `
<div class="grid"></div><div class="glow"></div>
<div class="left">
	<div class="kicker mono"><div class="dot"></div>central de comando</div>
	<div class="word mono">DC<b>C</b></div>
	<div class="tag">Seus agentes de IA,<br><span>no seu bolso.</span></div>
</div>
<div class="right">
	<div class="card wait">${robot("claude", "waiting", 64)}<div><div class="t">loja-online</div><div class="pill mono" style="color:${C.wait}"><i style="background:${C.wait}"></i>precisa de você</div></div></div>
	<div class="card">${robot("codex", "working", 64, "move")}<div><div class="t">api-pagamentos</div><div class="pill mono" style="color:${C.accent}"><i style="background:${C.accent}"></i>rodando</div></div></div>
	<div class="fauna">${bicho("capivara", "emerald", 56)}${bicho("tucano", "orange", 56, "move")}${bicho("onca", "lime", 56)}${bicho("polvo", "violet", 56)}${bicho("boto", "rose", 56)}</div>
</div>`;
	return page("Recurso gráfico", css, body);
}

// ---------------------------------------------------------------------------
// Captioned screenshots. One caption per raw print; the print sits under the
// caption in a rounded frame, whole (the composer and tab bar stay visible).
// ---------------------------------------------------------------------------

const SHOTS = [
	{ file: "01-agentes.png", title: "Veja quem <em>precisa de você</em>", sub: "Todos os agentes do seu computador, num relance" },
	{ file: "02-aprovacao.png", title: "Aprove com <em>um toque</em>", sub: "Permissões e perguntas chegam no celular" },
	{ file: "03-nova-tarefa.png", title: "Delegue de <em>qualquer lugar</em>", sub: "Escolha o agente, o acesso e o worktree" },
	{ file: "04-projetos.png", title: "Seus projetos, <em>à mão</em>", sub: "O que espera e o que roda em cada um" },
	{ file: "05-mudancas.png", title: "Revise o <em>diff</em> no bolso", sub: "Arquivo por arquivo, e commit dali mesmo" },
];

/**
 * `crop` trims the top of the print in raw pixels: the Play set drops the iOS
 * status bar so the phone image doesn't read as an iPhone.
 */
function captioned({ width, height, shot, raw, rawW, rawH, crop, scale }) {
	const s = scale; // layout unit: 1 at 1320 wide
	const visibleH = rawH - crop;
	const captionH = Math.round(560 * s * (height / width > 2 ? 1 : 0.78));
	const maxH = height - captionH - Math.round(70 * s);
	const maxW = width - Math.round(150 * s);
	const k = Math.min(maxH / visibleH, maxW / rawW);
	const w = Math.round(rawW * k);
	const h = Math.round(visibleH * k);
	const radius = Math.round((crop ? 60 : 165) * k);
	const css = `
body { width: ${width}px; height: ${height}px; position: relative; }
.grid { background-size: ${Math.round(66 * s)}px ${Math.round(66 * s)}px; -webkit-mask-image: radial-gradient(ellipse at 50% 0%, #000 0%, transparent 70%); }
.glow { position: absolute; left: 50%; top: ${captionH - Math.round(120 * s)}px; width: ${Math.round(1500 * s)}px; height: ${Math.round(1500 * s)}px; transform: translateX(-50%); border-radius: 50%;
	background: radial-gradient(circle, ${C.accent}1f 0%, transparent 60%); }
.cap { position: absolute; left: ${Math.round(80 * s)}px; right: ${Math.round(80 * s)}px; top: 0; height: ${captionH}px; display: flex; flex-direction: column; justify-content: center; align-items: center; text-align: center; }
.kicker { display: flex; align-items: center; gap: ${Math.round(14 * s)}px; font-size: ${Math.round(30 * s)}px; letter-spacing: ${Math.round(5 * s)}px; color: ${C.mute}; text-transform: uppercase; margin-bottom: ${Math.round(28 * s)}px; }
.kicker b { color: ${C.fg}; letter-spacing: ${Math.round(6 * s)}px; }
.dot { width: ${Math.round(18 * s)}px; height: ${Math.round(18 * s)}px; border-radius: 50%; background: ${C.accent}; box-shadow: 0 0 ${Math.round(20 * s)}px ${C.accent}; }
h1 { font-size: ${Math.round(108 * s)}px; line-height: 1.04; font-weight: 800; letter-spacing: ${(-1.5 * s).toFixed(1)}px; }
h1 em { font-style: normal; color: ${C.accent}; }
p { margin-top: ${Math.round(24 * s)}px; font-size: ${Math.round(46 * s)}px; line-height: 1.2; color: ${C.mute}; font-weight: 500; }
.phone { position: absolute; left: ${Math.round((width - w) / 2)}px; top: ${captionH}px; width: ${w}px; height: ${h}px; border-radius: ${radius}px; overflow: hidden;
	box-shadow: 0 0 0 ${Math.max(3, Math.round(6 * s))}px #1d222b, 0 0 0 ${Math.max(4, Math.round(8 * s))}px #2c3340, 0 ${Math.round(40 * s)}px ${Math.round(120 * s)}px #000c; }
.phone img { display: block; width: ${w}px; height: ${Math.round(rawH * k)}px; margin-top: ${-Math.round(crop * k)}px; }
`;
	const body = `
<div class="grid"></div><div class="glow"></div>
<div class="cap">
	<div class="kicker mono"><div class="dot"></div><b>DCC</b> central de comando</div>
	<h1>${shot.title}</h1>
	<p>${shot.sub}</p>
</div>
<div class="phone"><img src="${raw}"></div>`;
	return page(shot.file, css, body);
}

// ---------------------------------------------------------------------------

function sizeOf(path) {
	const out = execFileSync("sips", ["-g", "pixelWidth", "-g", "pixelHeight", path], { encoding: "utf8" });
	return { w: Number(/pixelWidth: (\d+)/.exec(out)[1]), h: Number(/pixelHeight: (\d+)/.exec(out)[1]) };
}

rmSync(work, { recursive: true, force: true });
mkdirSync(work, { recursive: true });
const renderer = join(work, "render");
execFileSync("swiftc", ["-O", join(here, "render.swift"), "-o", renderer], { stdio: "inherit" });

function render(name, html, width, height, out) {
	const file = join(work, `${name}.html`);
	writeFileSync(file, html);
	mkdirSync(dirname(out), { recursive: true });
	execFileSync(renderer, [file, String(width), String(height), out], { stdio: "inherit" });
	console.log(`${relative(root, out)}  ${width}×${height}`);
}

render("feature-graphic", featureGraphic(), 1024, 500, join(root, "google-play/feature-graphic-1024x500.png"));

const rawDir = join(root, "app-store/raw");
for (const shot of SHOTS) {
	const rawPath = join(rawDir, shot.file);
	if (!existsSync(rawPath)) {
		console.warn(`skip ${shot.file}: no raw print in app-store/raw/`);
		continue;
	}
	const { w: rawW, h: rawH } = sizeOf(rawPath);
	const raw = relative(work, rawPath);
	const base = shot.file.replace(/\.png$/, "");
	render(`ios-${base}`, captioned({ width: 1320, height: 2868, shot, raw, rawW, rawH, crop: 0, scale: 1 }), 1320, 2868, join(root, `app-store/iphone-6.9/${shot.file}`));
	// App Store Connect's "iPhone with Dynamic Island (medium display)" slot takes 6.3" (1206×2622).
	render(`ios63-${base}`, captioned({ width: 1206, height: 2622, shot, raw, rawW, rawH, crop: 0, scale: 1206 / 1320 }), 1206, 2622, join(root, `app-store/iphone-6.3/${shot.file}`));
	// The Play set drops the iOS status bar (top 62 pt at 3×).
	render(`play-${base}`, captioned({ width: 1080, height: 1920, shot, raw, rawW, rawH, crop: 186, scale: 1080 / 1320 }), 1080, 1920, join(root, `google-play/phone/${shot.file}`));
}

rmSync(work, { recursive: true, force: true });
