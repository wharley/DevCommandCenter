/**
 * The desktop's pixel sprites (`@dcc/mascots`) on the phone: the agent robot
 * per provider and the project's Brazilian-fauna glyph. Same drawings, same
 * rules — the robot faces you when idle, walks sideways while the agent works
 * and waves when it needs you; the project mascot animates while an agent
 * runs there.
 */
import {
	agentMascotPaths,
	projectMascotPaths,
	resolveAgentMascotKey,
	resolveProjectColor,
	resolveProjectIcon,
	type AgentMascotKey,
	type AgentMascotState,
	type MascotPaths,
	type ProjectColorId,
	type ProjectSymbolId,
} from "@dcc/mascots";
import Feather from "@expo/vector-icons/Feather";
import { useEffect, useState } from "react";
import { Image, View } from "react-native";
import { useReducedMotion } from "react-native-reanimated";
import Svg, { Path } from "react-native-svg";
import { alpha, useTheme, type Palette, type Scheme } from "../theme";

/** Half of the desktop's 0.9 s cycle: one frame each. */
const FRAME_MS = 450;

/** `+` pixels: the sprite color mixed halfway to white (the desktop's dark theme). */
function lighter(hex: string): string {
	const n = Number.parseInt(hex.slice(1, 7), 16);
	const mix = (c: number) => Math.round(c + (255 - c) / 2);
	const r = mix((n >> 16) & 255);
	const g = mix((n >> 8) & 255);
	const b = mix(n & 255);
	return `#${((r << 16) | (g << 8) | b).toString(16).padStart(6, "0")}`;
}

export function PixelSprite({
	paths,
	active,
	size,
	tint,
}: {
	paths: MascotPaths;
	/** Alternates the `rest` and `move` frames. */
	active: boolean;
	/** A multiple of 10 keeps every pixel on whole device pixels (no anti-aliased seams). */
	size: number;
	tint: string;
}) {
	const reduceMotion = useReducedMotion();
	const animate = active && !reduceMotion;
	const [moving, setMoving] = useState(false);
	useEffect(() => {
		if (!animate) {
			setMoving(false);
			return;
		}
		const timer = setInterval(() => setMoving((value) => !value), FRAME_MS);
		return () => clearInterval(timer);
	}, [animate]);
	const frame = animate && moving ? paths.move : paths.rest;
	return (
		<Svg width={size} height={size} viewBox="0 0 10 10">
			<Path d={frame.body} fill={tint} />
			{frame.accent ? <Path d={frame.accent} fill={lighter(tint)} /> : null}
		</Svg>
	);
}

type Tile = { base: string; ink: string };

/** Brand color where the brand has one; the monochrome marks stay neutral (`null`). */
const AGENT_TILES: Record<AgentMascotKey, Tile | null> = {
	claude: { base: "#D97757", ink: "#E8936F" },
	codex: { base: "#3B7BF6", ink: "#7AA5FA" },
	gemini: { base: "#8B5CF6", ink: "#C4B5FD" },
	cursor: null,
	droid: null,
	grok: null,
	generic: null,
};

/**
 * The sprite color on its tile. Dark follows the desktop (light ink on a
 * faint tile); light draws in the tile's own color so it holds on white.
 */
function agentTile(key: AgentMascotKey, color: Palette, scheme: Scheme): Tile {
	const tile = AGENT_TILES[key];
	if (!tile) return { base: color.mute, ink: scheme === "dark" && key !== "generic" ? "#c4cbd4" : color.mute };
	return scheme === "dark" ? tile : { base: tile.base, ink: tile.base };
}

/** The agent's face: a pixel robot in its provider's tile; amber (and waving) when it needs you. */
export function AgentMascot({
	provider,
	model,
	state = "idle",
	size = 24,
}: {
	provider?: string | null;
	model?: string | null;
	state?: AgentMascotState;
	size?: number;
}) {
	const { color, scheme } = useTheme();
	const key = resolveAgentMascotKey(provider, model);
	const waiting = state === "waiting";
	const tile = agentTile(key, color, scheme);
	const ink = waiting ? color.wait : tile.ink;
	return (
		<View
			accessible={false}
			importantForAccessibility="no-hide-descendants"
			style={{
				width: size,
				height: size,
				borderRadius: Math.round(size / 4),
				borderWidth: 1,
				borderColor: waiting ? color.wait : alpha(tile.base, 0.3),
				backgroundColor: alpha(waiting ? color.wait : tile.base, waiting ? 0.15 : 0.12),
				alignItems: "center",
				justifyContent: "center",
			}}
		>
			<PixelSprite
				paths={agentMascotPaths(key, state)}
				active={state !== "idle"}
				size={Math.round((size * 0.8) / 10) * 10 || 10}
				tint={ink}
			/>
		</View>
	);
}

/** Tailwind 500 (tile) and 300 (ink) of each project color, as the desktop's dark theme. */
const PROJECT_TILES: Record<ProjectColorId, Tile> = {
	slate: { base: "#64748b", ink: "#cbd5e1" },
	sky: { base: "#0ea5e9", ink: "#7dd3fc" },
	cyan: { base: "#06b6d4", ink: "#67e8f9" },
	emerald: { base: "#10b981", ink: "#6ee7b7" },
	amber: { base: "#f59e0b", ink: "#fcd34d" },
	orange: { base: "#f97316", ink: "#fdba74" },
	rose: { base: "#f43f5e", ink: "#fda4af" },
	violet: { base: "#8b5cf6", ink: "#c4b5fd" },
	indigo: { base: "#6366f1", ink: "#a5b4fc" },
	fuchsia: { base: "#d946ef", ink: "#f0abfc" },
	lime: { base: "#84cc16", ink: "#bef264" },
	pink: { base: "#ec4899", ink: "#f9a8d4" },
};

const SYMBOL_ICONS: Record<ProjectSymbolId, keyof typeof Feather.glyphMap> = {
	folder: "folder",
	terminal: "terminal",
	code: "code",
	layers: "layers",
	package: "package",
	database: "database",
	globe: "globe",
	rocket: "send",
	branch: "git-branch",
	cpu: "cpu",
	shield: "shield",
	wrench: "tool",
};

/** The project's identity, the same one the desktop sidebar shows (mascot, symbol or logo). */
export function ProjectGlyph({
	icon,
	color: colorValue,
	seed,
	active = false,
	size = 28,
}: {
	icon?: string | null;
	color?: string | null;
	/** The project's root path: picks the automatic mascot and color. */
	seed?: string | null;
	/** An agent is running in the project: the mascot animates. */
	active?: boolean;
	size?: number;
}) {
	const { scheme } = useTheme();
	const visual = resolveProjectIcon(icon, seed);
	const palette = PROJECT_TILES[resolveProjectColor(colorValue, seed)];
	// Light draws in the 500 shade: the 300 ink washes out on white.
	const tile = scheme === "dark" ? palette : { base: palette.base, ink: palette.base };
	const inner = Math.round((size * 0.6) / 10) * 10 || 10;
	return (
		<View
			accessible={false}
			importantForAccessibility="no-hide-descendants"
			style={{
				width: size,
				height: size,
				borderRadius: Math.round(size / 3.5),
				borderWidth: 1,
				borderColor: alpha(tile.base, 0.3),
				backgroundColor: alpha(tile.base, 0.12),
				alignItems: "center",
				justifyContent: "center",
				overflow: "hidden",
			}}
		>
			{visual.kind === "mascot" ? (
				<PixelSprite paths={projectMascotPaths(visual.id)} active={active} size={inner} tint={tile.ink} />
			) : visual.kind === "icon" ? (
				<Feather name={SYMBOL_ICONS[visual.id]} size={Math.round(size * 0.5)} color={tile.ink} />
			) : (
				<Image source={{ uri: visual.src }} style={{ width: size * 0.8, height: size * 0.8, borderRadius: 3 }} />
			)}
		</View>
	);
}
