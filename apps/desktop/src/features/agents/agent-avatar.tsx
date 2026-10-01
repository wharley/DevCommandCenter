import type { AgentAvatar as AgentAvatarSpec, AgentAvatarColor } from "@/lib/agents-api";
import { cn } from "@/lib/utils";
import "./agents.css";

/** What the agent is doing, shown on the mascot itself. */
export type AgentActivityState = "idle" | "working" | "needsYou" | "done";

const INK = "#0b1220";
const COLORS: Record<AgentAvatarColor, string> = {
	blue: "#356EFF",
	cyan: "#20BEFF",
	violet: "#8B5CF6",
	amber: "#F2A93B",
	green: "#34C28A",
	pink: "#F472B6",
};

type EyeStyle = AgentAvatarSpec["eyes"] | "focus" | "resting";

function Eyes({ style }: { style: EyeStyle }) {
	if (style === "smile") {
		return (
			<path
				d="M32 48q7 -9 14 0M54 48q7 -9 14 0"
				fill="none"
				stroke={INK}
				strokeWidth="4.5"
				strokeLinecap="round"
			/>
		);
	}
	if (style === "resting") {
		return (
			<path d="M32 46h14M54 46h14" fill="none" stroke={INK} strokeWidth="4.5" strokeLinecap="round" />
		);
	}
	if (style === "visor") {
		return (
			<>
				<rect x="27" y="38" width="46" height="16" rx="8" fill={INK} />
				<circle cx="40" cy="46" r="3.2" fill="#fff" />
				<circle cx="60" cy="46" r="3.2" fill="#fff" />
			</>
		);
	}
	const pupilX = style === "focus" ? 0 : 1.5;
	const pupilY = style === "focus" ? 50 : 47;
	return (
		<>
			<ellipse cx="39" cy="46" rx="7.5" ry="8.5" fill="#fff" />
			<ellipse cx="61" cy="46" rx="7.5" ry="8.5" fill="#fff" />
			<circle cx={39 + pupilX} cy={pupilY} r="3.8" fill={INK} />
			<circle cx={61 + pupilX} cy={pupilY} r="3.8" fill={INK} />
		</>
	);
}

/**
 * The DCC agent mascot. Colour, arm count and eyes identify the agent; when a
 * `state` is given the eyes, a raised arm and a badge show what it is doing.
 * Decorative by default: pass `label` when the avatar is the only name shown.
 */
export function AgentAvatar({
	avatar,
	state,
	size = 24,
	label,
	className,
}: {
	avatar: AgentAvatarSpec;
	state?: AgentActivityState;
	size?: number;
	label?: string;
	className?: string;
}) {
	const color = COLORS[avatar.color] ?? COLORS.blue;
	const arms = Math.min(5, Math.max(3, Math.round(avatar.arms)));
	const gap = 3;
	const width = (68 - gap * (arms - 1)) / arms;
	const eyes: EyeStyle =
		state === "idle"
			? "resting"
			: state === "working"
				? "focus"
				: state === "done"
					? "smile"
					: state === "needsYou"
						? "round"
						: avatar.eyes;
	return (
		<svg
			width={size}
			height={size}
			viewBox="0 0 100 100"
			className={cn(state === "working" && "dcc-agent-working", className)}
			role={label ? "img" : undefined}
			aria-label={label}
			aria-hidden={label ? undefined : true}
		>
			<g opacity={state === "idle" ? 0.6 : 1}>
				{state === "needsYou" && (
					<rect
						x="78"
						y="14"
						width="13"
						height="40"
						rx="6.5"
						fill={color}
						transform="rotate(24 84 52)"
					/>
				)}
				{Array.from({ length: arms }, (_, index) => (
					<rect
						// Arms are positional and never reorder.
						key={index}
						x={16 + index * (width + gap)}
						y="56"
						width={width}
						height={index % 2 === 0 ? 26 : 20}
						rx={width / 2}
						fill={color}
					/>
				))}
				<path d="M16 60V52C16 29 32 14 50 14s34 15 34 38v8z" fill={color} />
				<ellipse
					cx="36"
					cy="27"
					rx="9"
					ry="5"
					fill="#fff"
					opacity="0.22"
					transform="rotate(-28 36 27)"
				/>
				<Eyes style={eyes} />
			</g>
			{state === "needsYou" && (
				<>
					<circle cx="82" cy="80" r="15" fill="#F2A93B" />
					<path d="M82 72v9" stroke={INK} strokeWidth="4" strokeLinecap="round" />
					<circle cx="82" cy="87" r="2.3" fill={INK} />
				</>
			)}
			{state === "done" && (
				<>
					<circle cx="82" cy="80" r="15" fill="#34C28A" />
					<path
						d="M75.5 80.5l4.5 4.5 8-9"
						fill="none"
						stroke={INK}
						strokeWidth="4"
						strokeLinecap="round"
						strokeLinejoin="round"
					/>
				</>
			)}
		</svg>
	);
}
