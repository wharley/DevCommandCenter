/**
 * Fauna brasileira em pixel-art — o ícone padrão de cada projeto.
 *
 * Cada bicho é uma grade 10×10: `#` pinta a cor do projeto, `+` pinta a mesma
 * cor em tom mais claro (bico do tucano, casco do jabuti, pintas da onça) e
 * `.` fica transparente — olhos e frestas são buracos que deixam o fundo
 * aparecer.
 *
 * Dois quadros: `rest` e `move`, alternados enquanto há agente rodando no
 * projeto — a capivara mastiga, o tucano bate asa, o polvo mexe os braços.
 */
export const MASCOT_GRID = 10;

type MascotRows = readonly string[];

type MascotFrames = { rest: MascotRows; move: MascotRows };

const MASCOT_FRAMES = {
	capivara: {
		rest: [
			"..........",
			"......#.#.",
			".....#####",
			"..####.###",
			".#########",
			"##########",
			"#########.",
			"#########.",
			".##.##.##.",
			".#..#..#..",
		],
		move: [
			"..........",
			"......#.#.",
			".....#####",
			"..####.###",
			".#########",
			"#########+",
			"#########.",
			"#########.",
			".##.##.##.",
			"..#..#..#.",
		],
	},
	tucano: {
		rest: [
			"..........",
			"...###....",
			"..##.#++++",
			"..####++++",
			".#####.++.",
			".#####....",
			".#####....",
			"..####....",
			"...#.#....",
			"..##.##...",
		],
		move: [
			"..........",
			"...###....",
			"..##.#++++",
			"..####++++",
			"######.++.",
			"#.####....",
			"..####....",
			"..####....",
			"...#.#....",
			"..##.##...",
		],
	},
	arara: {
		rest: [
			".....###..",
			"....##.#+.",
			"....####++",
			"....####.+",
			"...#####..",
			"...+####..",
			"..++###...",
			"..+###....",
			".##.#.....",
			"##........",
		],
		move: [
			".....###..",
			"....##.#+.",
			"....####++",
			"....####.+",
			"..++####..",
			".+++####..",
			"+..###....",
			"...##.....",
			"..##.#....",
			".##.......",
		],
	},
	mico: {
		rest: [
			"..######..",
			".##++++##.",
			"##+.++.+##",
			"##++++++##",
			".##+..+##.",
			"..######..",
			"...####..#",
			"...####.#.",
			"...#..##..",
			"..##..#...",
		],
		move: [
			"..######..",
			".##++++##.",
			"##+.++.+##",
			"##++++++##",
			".##++++##.",
			"..######.#",
			"...####.#.",
			"...#####..",
			"...#..#...",
			"..##..##..",
		],
	},
	tatu: {
		rest: [
			"..........",
			"..........",
			"...####...",
			"..#+#+#.#.",
			".#+#+#+##.",
			".#+#+#+#.#",
			".#+#+#+###",
			"##########",
			"..##..##..",
			"..........",
		],
		move: [
			"..........",
			"..........",
			"...####...",
			"..#+#+#.#.",
			".#+#+#+##.",
			".#+#+#+#.#",
			".#+#+#+###",
			"##########",
			".##..##...",
			"..........",
		],
	},
	onca: {
		rest: [
			"..........",
			"......#.#.",
			"......####",
			"#.....#.##",
			"#.#######+",
			".#+##+###.",
			".##+##+#..",
			".########.",
			".#.#..#.#.",
			".#.#..#.#.",
		],
		move: [
			"..........",
			"......#.#.",
			"......####",
			"......#.##",
			"##.######+",
			"..+##+###.",
			".##+##+#..",
			".########.",
			"#.#....#.#",
			"..........",
		],
	},
	jabuti: {
		rest: [
			"..........",
			"..........",
			"..####....",
			".#++++#...",
			"#+#++#+#..",
			"#++##++###",
			"#########.",
			".##..##...",
			"..........",
			"..........",
		],
		move: [
			"..........",
			"..........",
			"..####....",
			".#++++#...",
			"#+#++#+#.#",
			"#++##++###",
			"#########.",
			"##..##....",
			"..........",
			"..........",
		],
	},
	boto: {
		rest: [
			"..........",
			"..........",
			"..........",
			"......##..",
			"....###.#.",
			"#.#######.",
			"##.+++++##",
			"#.........",
			"..........",
			"..........",
		],
		move: [
			"..........",
			"..........",
			"......##..",
			"#...###.#.",
			"#.#######.",
			".#.+++++##",
			"..........",
			"..........",
			"..........",
			"..........",
		],
	},
	sapo: {
		rest: [
			"..........",
			".###..###.",
			"##.####.##",
			"##########",
			"#+######+#",
			"##++++++##",
			".########.",
			".#.#..#.#.",
			"##......##",
			"..........",
		],
		move: [
			".###..###.",
			"##.####.##",
			"##########",
			"#+######+#",
			"##++++++##",
			".########.",
			"#.#....#.#",
			"#........#",
			"..........",
			"..........",
		],
	},
	polvo: {
		rest: [
			"...####...",
			"..######..",
			".########.",
			".#..##..#.",
			".#..##..#.",
			".########.",
			".########.",
			"#.#.##.#.#",
			"#.#.#..#.#",
			".#..#.#..#",
		],
		move: [
			"...####...",
			"..######..",
			".########.",
			".#..##..#.",
			".#..##..#.",
			".########.",
			".########.",
			".#.#..#.#.",
			"#..#.#..#.",
			"#.#..#.#..",
		],
	},
} as const satisfies Record<string, MascotFrames>;

export type ProjectMascotId = keyof typeof MASCOT_FRAMES;

export const PROJECT_MASCOT_IDS = Object.keys(MASCOT_FRAMES) as ProjectMascotId[];

export function isProjectMascotId(value: string | null | undefined): value is ProjectMascotId {
	return typeof value === "string" && Object.hasOwn(MASCOT_FRAMES, value);
}

/** Path data over a 10×10 viewBox: `body` (`#`) and `accent` (`+`). */
export type MascotFramePaths = { body: string; accent: string };

export type MascotPaths = { rest: MascotFramePaths; move: MascotFramePaths };

/** Merges each row's horizontal runs of `glyph` into one rect per run. */
function rowsToPath(rows: MascotRows, glyph: "#" | "+"): string {
	let d = "";
	rows.forEach((row, y) => {
		let x = 0;
		while (x < row.length) {
			if (row[x] !== glyph) {
				x += 1;
				continue;
			}
			const start = x;
			while (x < row.length && row[x] === glyph) x += 1;
			d += `M${start} ${y}h${x - start}v1h${start - x}z`;
		}
	});
	return d;
}

function framePaths(rows: MascotRows): MascotFramePaths {
	return { body: rowsToPath(rows, "#"), accent: rowsToPath(rows, "+") };
}

const pathCache = new Map<ProjectMascotId, MascotPaths>();

export function projectMascotPaths(id: ProjectMascotId): MascotPaths {
	let paths = pathCache.get(id);
	if (!paths) {
		const frames: MascotFrames = MASCOT_FRAMES[id];
		paths = { rest: framePaths(frames.rest), move: framePaths(frames.move) };
		pathCache.set(id, paths);
	}
	return paths;
}

/** Stable 32-bit FNV-1a over the seed (usually the project root path). */
export function projectSeedHash(seed: string): number {
	let hash = 0x811c9dc5;
	for (let index = 0; index < seed.length; index += 1) {
		hash ^= seed.charCodeAt(index);
		hash = Math.imul(hash, 0x01000193);
	}
	return hash >>> 0;
}

export function autoProjectMascot(seed: string): ProjectMascotId {
	return PROJECT_MASCOT_IDS[projectSeedHash(seed) % PROJECT_MASCOT_IDS.length]!;
}

// ---------------------------------------------------------------------------
// Project identity rules shared by every DCC surface (desktop, mobile web):
// the same project must get the same mascot and color everywhere.
// ---------------------------------------------------------------------------

/** Symbol ids a person can pick instead of a mascot (rendered per app). */
export const PROJECT_SYMBOL_IDS = [
	"folder",
	"terminal",
	"code",
	"layers",
	"package",
	"database",
	"globe",
	"rocket",
	"branch",
	"cpu",
	"shield",
	"wrench",
] as const;

export type ProjectSymbolId = (typeof PROJECT_SYMBOL_IDS)[number];

export const PROJECT_COLOR_IDS = [
	"slate",
	"sky",
	"cyan",
	"emerald",
	"amber",
	"orange",
	"rose",
	"violet",
	"indigo",
	"fuchsia",
	"lime",
	"pink",
] as const;

export type ProjectColorId = (typeof PROJECT_COLOR_IDS)[number];

/** Colors the auto pick draws from: slate reads as "unset", amber is "needs you". */
const AUTO_COLOR_IDS = PROJECT_COLOR_IDS.filter((id) => id !== "slate" && id !== "amber");

/** Fallback mascot when a caller has no seed to hash. */
const FALLBACK_MASCOT: ProjectMascotId = "polvo";

export type ProjectIconVisual =
	| { kind: "mascot"; id: ProjectMascotId }
	| { kind: "icon"; id: ProjectSymbolId }
	| { kind: "logo"; src: string };

const LOGO_PREFIX = "data:image/png;base64,";

/** A project's icon value for rendering: its uploaded logo when that is the pick. */
export function projectIconValue(project: {
	icon?: string | null;
	logo?: string | null;
}): string | null {
	return project.icon === "logo" && project.logo ? project.logo : (project.icon ?? null);
}

export function isProjectSymbolId(value: string | null | undefined): value is ProjectSymbolId {
	return PROJECT_SYMBOL_IDS.includes(value as ProjectSymbolId);
}

export function isProjectColorId(value: string | null | undefined): value is ProjectColorId {
	return PROJECT_COLOR_IDS.includes(value as ProjectColorId);
}

/**
 * An explicit pick (mascot or symbol) wins; anything else — null, empty,
 * unknown — falls back to the mascot hashed from the seed (the project path).
 */
export function resolveProjectIcon(
	value: string | null | undefined,
	seed?: string | null,
): ProjectIconVisual {
	if (isProjectMascotId(value)) return { kind: "mascot", id: value };
	if (isProjectSymbolId(value)) return { kind: "icon", id: value };
	if (value?.startsWith(LOGO_PREFIX)) return { kind: "logo", src: value };
	return { kind: "mascot", id: seed ? autoProjectMascot(seed) : FALLBACK_MASCOT };
}

export function resolveProjectColor(
	value: string | null | undefined,
	seed?: string | null,
): ProjectColorId {
	if (isProjectColorId(value)) return value;
	if (!seed) return "slate";
	return AUTO_COLOR_IDS[projectSeedHash(`color:${seed}`) % AUTO_COLOR_IDS.length]!;
}

// ---------------------------------------------------------------------------
// Agent robots — the face of each provider in the conversation timeline.
//
// A pixel robot. It faces you while the turn is over or while it needs you;
// once the agent works it turns sideways and walks right, toward the model
// label it sits next to. Every provider shares the body; the antenna is what
// tells them apart without relying on color alone. Same grammar as the fauna: `#`
// paints the provider color, `+` a lighter tone (the near arm, antenna tips).
// ---------------------------------------------------------------------------

export const AGENT_MASCOT_KEYS = [
	"claude",
	"codex",
	"gemini",
	"cursor",
	"droid",
	"grok",
	"generic",
] as const;

export type AgentMascotKey = (typeof AGENT_MASCOT_KEYS)[number];

/** `idle`: faces you, still. `working`: walks right. `waiting`: faces you and waves. */
export type AgentMascotState = "idle" | "working" | "waiting";

/** Rows 0–1: the antenna, centered over the head. */
const AGENT_ANTENNAS: Record<AgentMascotKey, readonly [string, string]> = {
	// Claude: a three-spike crown, a nod to the burst.
	claude: ["..+.+.+...", "...###...."],
	// Codex: a round bulb.
	codex: ["....++....", ".....#...."],
	// Gemini: a four-point sparkle.
	gemini: ["....+.....", "...+#+...."],
	// Cursor: a T-shaped mast.
	cursor: ["...+++....", "....#....."],
	// Droid: two ears in a V.
	droid: ["..+...+...", "...#.#...."],
	// Grok: a slanted slash.
	grok: [".....+....", "....#....."],
	generic: ["....+.....", "....#....."],
};

/** Rows 2–4 seen from the side: one eye near the front, so it faces right. */
const AGENT_HEAD = ["..######..", "..####.#..", "..######.."] as const;

/** Rows 2–9 for each pose, before the antenna is laid on top. */
const AGENT_POSES = {
	front: [
		"..######..",
		"..#.##.#..",
		"..######..",
		"..+####+..",
		"..+####+..",
		"...####...",
		"...#..#...",
		"..##..##..",
	],
	frontHandUp: [
		"..######..",
		"..#.##.#.+",
		"..######+.",
		"..+####+..",
		"..+####...",
		"...####...",
		"...#..#...",
		"..##..##..",
	],
	frontHandHigh: [
		"..######.+",
		"..#.##.#.+",
		"..######+.",
		"..+####+..",
		"..+####...",
		"...####...",
		"...#..#...",
		"..##..##..",
	],
	stand: [
		...AGENT_HEAD,
		"...#+##...",
		"...#+##...",
		"...####...",
		"...#..#...",
		"...##.##..",
	],
	stride: [
		...AGENT_HEAD,
		"...##+#...",
		"...####+..",
		"...####...",
		"..#....#..",
		"..##...##.",
	],
} as const satisfies Record<string, readonly string[]>;

function agentFrame(key: AgentMascotKey, pose: keyof typeof AGENT_POSES): MascotRows {
	return [...AGENT_ANTENNAS[key], ...AGENT_POSES[pose]];
}

const AGENT_STATE_POSES: Record<
	AgentMascotState,
	{ rest: keyof typeof AGENT_POSES; move: keyof typeof AGENT_POSES }
> = {
	idle: { rest: "front", move: "front" },
	working: { rest: "stand", move: "stride" },
	waiting: { rest: "frontHandUp", move: "frontHandHigh" },
};

export function isAgentMascotKey(value: string | null | undefined): value is AgentMascotKey {
	return typeof value === "string" && (AGENT_MASCOT_KEYS as readonly string[]).includes(value);
}

const agentPathCache = new Map<string, MascotPaths>();

/** Two frames for the robot in `state`; `idle` repeats the same pose. */
export function agentMascotPaths(key: AgentMascotKey, state: AgentMascotState): MascotPaths {
	const cacheKey = `${key}:${state}`;
	let paths = agentPathCache.get(cacheKey);
	if (!paths) {
		const poses = AGENT_STATE_POSES[state];
		paths = {
			rest: framePaths(agentFrame(key, poses.rest)),
			move: framePaths(agentFrame(key, poses.move)),
		};
		agentPathCache.set(cacheKey, paths);
	}
	return paths;
}

/** Raw rows of both frames for every key in `state` — for tests on the grid. */
export function agentMascotRows(state: AgentMascotState): MascotRows[] {
	const poses = AGENT_STATE_POSES[state];
	return AGENT_MASCOT_KEYS.flatMap((key) => [
		agentFrame(key, poses.rest),
		agentFrame(key, poses.move),
	]);
}
