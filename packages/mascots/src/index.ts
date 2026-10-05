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
	| { kind: "icon"; id: ProjectSymbolId };

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
